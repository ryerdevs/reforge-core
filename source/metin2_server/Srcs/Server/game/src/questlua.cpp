#include "stdafx.h"

#include <sstream>

#include "questmanager.h"
#include "questlua.h"
#include "config.h"
#include "desc.h"
#include "char.h"
#include "char_manager.h"
#include "buffer_manager.h"
#include "db.h"
#include "xmas_event.h"
#include "locale_service.h"
#include "regen.h"
#include "affect.h"
#include "guild.h"
#include "guild_manager.h"
#include "sectree_manager.h"

#undef sys_err
#ifndef __WIN32__
#define sys_err(fmt, args...) quest::CQuestManager::instance().QuestError(__FUNCTION__, __LINE__, fmt, ##args)
#else
#define sys_err(fmt, ...) quest::CQuestManager::instance().QuestError(__FUNCTION__, __LINE__, fmt, __VA_ARGS__)
#endif

namespace quest
{
	using namespace std;

	string ScriptToString(const string& str)
	{
		lua_State* L = CQuestManager::instance().GetLuaState();
		const int x = lua_gettop(L);

		const int errcode = lua_dobuffer(L, ("return "+str).c_str(), str.size()+7, "ScriptToString");
		string retstr;
		if (!errcode)
		{
			if (lua_isstring(L,-1))
				retstr = lua_tostring(L, -1);
		}
		else
		{
			sys_err("LUA ScriptRunError (code:%d src:[%s])", errcode, str.c_str());
		}
		lua_settop(L,x);
		return retstr;
	}

	void FSetWarpLocation::operator() (LPCHARACTER ch) const
	{
		if (ch->IsPC())
		{
			ch->SetWarpLocation (map_index, x, y);
		}
	}

	void FSetQuestFlag::operator() (LPCHARACTER ch) const
	{
		if (!ch->IsPC())
			return;

		PC * pPC = CQuestManager::instance().GetPCForce(ch->GetPlayerID());

		if (pPC)
			pPC->SetFlag(flagname, value);
	}

	bool FPartyCheckFlagLt::operator() (LPCHARACTER ch) const
	{
		if (!ch->IsPC())
			return false;

		PC * pPC = CQuestManager::instance().GetPCForce(ch->GetPlayerID());
		bool returnBool = false;
		if (pPC)
		{
			const int flagValue = pPC->GetFlag(flagname);
			if (value > flagValue)
				returnBool = true;
			else
				returnBool = false;
		}

		return returnBool;
	}

	FPartyChat::FPartyChat(int ChatType, const char* str) : iChatType(ChatType), str(str)
	{
	}

	void FPartyChat::operator() (LPCHARACTER ch) const
	{
		ch->ChatPacket(iChatType, "%s", str);
	}

	void FPartyClearReady::operator() (LPCHARACTER ch) const
	{
		ch->RemoveAffect(AFFECT_DUNGEON_READY);
	}

	void FSendPacket::operator() (LPENTITY ent) const
	{
		if (ent->IsType(ENTITY_CHARACTER))
		{
			const auto ch = (LPCHARACTER) ent;

			if (ch->GetDesc())
			{
				ch->GetDesc()->Packet(buf.read_peek(), buf.size());
			}
		}
	}

#ifdef ENABLE_NEWSTUFF
	void FSendChatPacket::operator() (LPENTITY ent) const
	{
		if (ent->IsType(ENTITY_CHARACTER))
		{
			const auto ch = (LPCHARACTER) ent;
			ch->ChatPacket(m_chat_type, "%s", m_text.c_str());
		}
	}
#endif

	void FSendPacketToEmpire::operator() (LPENTITY ent) const
	{
		if (ent->IsType(ENTITY_CHARACTER))
		{
			const auto ch = (LPCHARACTER) ent;

			if (ch->GetDesc())
			{
				if (ch->GetEmpire() == bEmpire)
					ch->GetDesc()->Packet(buf.read_peek(), buf.size());
			}
		}
	}

	void FWarpEmpire::operator() (LPENTITY ent) const
	{
		if (ent->IsType(ENTITY_CHARACTER))
		{
			const auto ch = (LPCHARACTER) ent;

			if (ch->IsPC() && ch->GetEmpire() == m_bEmpire)
			{
				ch->WarpSet(m_x, m_y, m_lMapIndexTo);
			}
		}
	}

	FBuildLuaGuildWarList::FBuildLuaGuildWarList(lua_State * lua_state) : L(lua_state), m_count(1)
	{
		lua_newtable(lua_state);
	}

	void FBuildLuaGuildWarList::operator() (DWORD g1, DWORD g2)
	{
		CGuild* g = CGuildManager::instance().FindGuild(g1);

		if (!g)
			return;

		if (g->GetGuildWarType(g2) == GUILD_WAR_TYPE_FIELD)
			return;

		if (g->GetGuildWarState(g2) != GUILD_WAR_ON_WAR)
			return;

		lua_newtable(L);
		lua_pushnumber(L, g1);
		lua_rawseti(L, -2, 1);
		lua_pushnumber(L, g2);
		lua_rawseti(L, -2, 2);
		lua_rawseti(L, -2, m_count++);
	}

	bool IsScriptTrue(const char* code, int size)
	{
		if (size==0)
			return true;

		lua_State* L = CQuestManager::instance().GetLuaState();
		const int x = lua_gettop(L);
		const int errcode = lua_dobuffer(L, code, size, "IsScriptTrue");
		const int bStart = lua_toboolean(L, -1);
		if (errcode)
		{
			char buf[100];
			snprintf(buf, sizeof(buf), "LUA ScriptRunError (code:%%d src:[%%%ds])", size);
			sys_err(buf, errcode, code);
		}
		lua_settop(L,x);
		return bStart != 0;
	}

	void combine_lua_string(lua_State * L, ostringstream & s)
	{
		char buf[32];

		const int n = lua_gettop(L);
		int i;

		for (i = 1; i <= n; ++i)
		{
			if (lua_isstring(L,i))
				//printf("%s\n",lua_tostring(L,i));
				s << lua_tostring(L, i);
			else if (lua_isnumber(L, i))
			{
				snprintf(buf, sizeof(buf), "%.14g\n", lua_tonumber(L,i));
				s << buf;
			}
		}
	}

	// "member" Lua functions

	ALUA(member_chat)
	{
		ostringstream s;
		combine_lua_string(L, s);
		CQuestManager::Instance().GetCurrentPartyMember()->ChatPacket(CHAT_TYPE_TALKING, "%s", s.str().c_str());
		return 0;
	}

	ALUA(member_clear_ready)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentPartyMember();
		ch->RemoveAffect(AFFECT_DUNGEON_READY);
		return 0;
	}

	ALUA(member_set_ready)
	{
		const LPCHARACTER ch = CQuestManager::instance().GetCurrentPartyMember();
		ch->AddAffect(AFFECT_DUNGEON_READY, POINT_NONE, 0, AFF_DUNGEON_READY, 65535, 0, true);
		return 0;
	}

	ALUA(mob_spawn)
	{
		if (!lua_isnumber(L, 1) || !lua_isnumber(L, 2) || !lua_isnumber(L, 3) || !lua_isnumber(L, 4))
		{
			sys_err("invalid argument");
			return 0;
		}

		const DWORD mob_vnum = (DWORD)lua_tonumber(L, 1);
		const long local_x = (long) lua_tonumber(L, 2)*100;
		const long local_y = (long) lua_tonumber(L, 3)*100;
		const float radius = (float) lua_tonumber(L, 4)*100;
		const bool bAggressive = lua_toboolean(L, 5);
		DWORD count = (lua_isnumber(L, 6))?(DWORD) lua_tonumber(L, 6):1;

		if (count == 0)
			count = 1;
		else if (count > 10)
		{
			sys_err("count bigger than 10");
			count = 10;
		}

		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		const LPSECTREE_MAP pMap = SECTREE_MANAGER::instance().GetMap(ch->GetMapIndex());
		if (pMap == nullptr) {
			return 0;
		}
		const DWORD dwQuestIdx = CQuestManager::instance().GetCurrentPC()->GetCurrentQuestIndex();

		bool ret = false;
		LPCHARACTER mob = nullptr;

		while (count--)
		{
			for (int loop = 0; loop < 8; ++loop)
			{
				const float angle = number(0, 999) * M_PI * 2 / 1000;
				const float r = number(0, 999) * radius / 1000;

				const long x = local_x + pMap->m_setting.iBaseX + (long)(r * cos(angle));
				const long y = local_y + pMap->m_setting.iBaseY + (long)(r * sin(angle));

				mob = CHARACTER_MANAGER::instance().SpawnMob(mob_vnum, ch->GetMapIndex(), x, y, 0);

				if (mob)
					break;
			}

			if (mob)
			{
				if (bAggressive)
					mob->SetAggressive();

				mob->SetQuestBy(dwQuestIdx);

				if (!ret)
				{
					ret = true;
					lua_pushnumber(L, (DWORD) mob->GetVID());
				}
			}
		}

		if (!ret)
			lua_pushnumber(L, 0);

		return 1;
	}

	ALUA(mob_spawn_group)
	{
		if (!lua_isnumber(L, 1) || !lua_isnumber(L, 2) || !lua_isnumber(L, 3) || !lua_isnumber(L, 4) || !lua_isnumber(L, 6))
		{
			sys_err("invalid argument");
			lua_pushnumber(L, 0);
			return 1;
		}

		const DWORD group_vnum = (DWORD)lua_tonumber(L, 1);
		const long local_x = (long) lua_tonumber(L, 2) * 100;
		const long local_y = (long) lua_tonumber(L, 3) * 100;
		const float radius = (float) lua_tonumber(L, 4) * 100;
		const bool bAggressive = lua_toboolean(L, 5);
		DWORD count = (DWORD) lua_tonumber(L, 6);

		if (count == 0)
			count = 1;
		else if (count > 10)
		{
			sys_err("count bigger than 10");
			count = 10;
		}

		const LPCHARACTER ch = CQuestManager::instance().GetCurrentCharacterPtr();
		const LPSECTREE_MAP pMap = SECTREE_MANAGER::instance().GetMap(ch->GetMapIndex());
		if (pMap == nullptr) {
			lua_pushnumber(L, 0);
			return 1;
		}
		const DWORD dwQuestIdx = CQuestManager::instance().GetCurrentPC()->GetCurrentQuestIndex();

		bool ret = false;
		LPCHARACTER mob = nullptr;

		while (count--)
		{
			for (int loop = 0; loop < 8; ++loop)
			{
				const float angle = number(0, 999) * M_PI * 2 / 1000;
				const float r = number(0, 999)*radius/1000;

				const long x = local_x + pMap->m_setting.iBaseX + (long)(r * cos(angle));
				const long y = local_y + pMap->m_setting.iBaseY + (long)(r * sin(angle));

				mob = CHARACTER_MANAGER::instance().SpawnGroup(group_vnum, ch->GetMapIndex(), x, y, x, y, nullptr, bAggressive);

				if (mob)
					break;
			}

			if (mob)
			{
				mob->SetQuestBy(dwQuestIdx);

				if (!ret)
				{
					ret = true;
					lua_pushnumber(L, (DWORD) mob->GetVID());
				}
			}
		}

		if (!ret)
			lua_pushnumber(L, 0);

		return 1;
	}

	// global Lua functions

	// Registers Lua function table

	void CQuestManager::AddLuaFunctionTable(const char * c_pszName, luaL_reg * preg, bool bCheckIfExists) const
	{
#ifdef ENABLE_NEWSTUFF
		bool bIsExists = false;
		if (bCheckIfExists)
		{
			const int x = lua_gettop(L);
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				lua_settop(L, x);
				bIsExists = true;
			}
		}
		if (!bIsExists)
			lua_newtable(L);
#else
		lua_newtable(L);
#endif

		while ((preg->name))
		{
			lua_pushstring(L, preg->name);
			lua_pushcfunction(L, preg->func);
			lua_rawset(L, -3);
			preg++;
		}

		lua_setglobal(L, c_pszName);
	}

	void CQuestManager::AddLuaFunctionSubTable(const char * c_pszName, const char * c_pszSubName, luaL_reg * preg) const
	{
		// lua_State* L = CQuestManager::instance().GetLuaState();
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				sys_err("%s global index not found for %s", c_pszName, c_pszSubName);
				lua_settop(L, x);
				return;
			}
			lua_pushstring(L, c_pszSubName);
			{
				lua_newtable(L);
				while ((preg->name))
				{
					lua_pushstring(L, preg->name);
					lua_pushcfunction(L, preg->func);
					lua_rawset(L, -3);
					preg++;
				}
			}
			lua_rawset(L, -3);
			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}

#ifdef ENABLE_NEWSTUFF
	void CQuestManager::AppendLuaFunctionTable(const char * c_pszName, luaL_reg * preg, bool bForceCreation) const
	{
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				sys_err("%s global index not found (force=%d)", c_pszName, bForceCreation);
				lua_settop(L, x);
				if (bForceCreation)
					AddLuaFunctionTable(c_pszName, preg);
				return;
			}

			while ((preg->name))
			{
				lua_pushstring(L, preg->name);
				lua_pushcfunction(L, preg->func);
				lua_rawset(L, -3);
				preg++;
			}

			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}

	void CQuestManager::AddLuaConstantGlobal(const char * c_pszName, lua_Number lNumber, bool bOverwrite) const
	{
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (lua_isnumber(L, -1))
			{
				if (!bOverwrite)
				{
					sys_err("%s global index already defined", c_pszName);
					lua_settop(L, x);
					return;
				}
			}
			lua_pushnumber(L, lNumber);
			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}

	void CQuestManager::AddLuaConstantInTable(const char * c_pszName, const char * c_pszSubName, lua_Number lNumber, bool bForceCreation) const
	{
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				if (!bForceCreation)
				{
					sys_err("%s global index for %s already defined", c_pszName, c_pszSubName);
					lua_settop(L, x);
					return;
				}
				lua_newtable(L);
			}
			{
				lua_pushstring(L, c_pszSubName);
				lua_pushnumber(L, lNumber);
				lua_rawset(L, -3);
			}
			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}

	void CQuestManager::AddLuaConstantInTable(const char * c_pszName, const char * c_pszSubName, const char * szString, bool bForceCreation) const
	{
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				if (!bForceCreation)
				{
					sys_err("%s global index for %s already defined", c_pszName, c_pszSubName);
					lua_settop(L, x);
					return;
				}
				lua_newtable(L);
			}
			{
				lua_pushstring(L, c_pszSubName);
				lua_pushstring(L, szString);
				lua_rawset(L, -3);
			}
			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}

	void CQuestManager::AddLuaConstantSubTable(const char * c_pszName, const char * c_pszSubName, luaC_tab * preg) const
	{
		// lua_State* L = CQuestManager::instance().GetLuaState();
		const int x = lua_gettop(L);
		{
			lua_getglobal(L, c_pszName);
			if (!lua_istable(L, -1))
			{
				sys_err("%s global index not found for %s", c_pszName, c_pszSubName);
				lua_settop(L, x);
				return;
			}
			lua_pushstring(L, c_pszSubName);
			{
				lua_newtable(L);
				while ((preg->name))
				{
					lua_pushstring(L, preg->name);
					switch (preg->val.type)
					{
						case ETL_CFUN:
							lua_pushcfunction(L, preg->val.cfVal);
							break;
						case ETL_LNUM:
							lua_pushnumber(L, preg->val.lnVal);
							break;
						case ETL_LSTR:
							lua_pushstring(L, preg->val.lsVal);
							break;
						case ETL_NIL:
							lua_pushnil(L);
							break;
					}
					lua_rawset(L, -3);
					preg++;
				}
			}
			lua_rawset(L, -3);
			lua_setglobal(L, c_pszName);
		}
		lua_settop(L, x);
	}
#endif

	void CQuestManager::BuildStateIndexToName(const char* questName) const
	{
		const int x = lua_gettop(L);
		lua_getglobal(L, questName);

		if (lua_isnil(L,-1))
		{
			sys_err("QUEST wrong quest state file for quest %s",questName);
			lua_settop(L,x);
			return;
		}

		for (lua_pushnil(L); lua_next(L, -2);)
		{
			if (lua_isstring(L, -2) && lua_isnumber(L, -1))
			{
				lua_pushvalue(L, -2);
				lua_rawset(L, -4);
			}
			else
			{
				lua_pop(L, 1);
			}
		}

		lua_settop(L, x);
	}

	bool CQuestManager::InitializeLua()
	{
#if LUA_V == 503
		L = lua_open();

		luaopen_base(L);
		luaopen_table(L);
		luaopen_string(L);
		luaopen_math(L);
		//TEMP
		luaopen_io(L);
		luaopen_debug(L);
#elif LUA_V == 523
		L = luaL_newstate();

		luaL_openlibs(L);
		//luaopen_debug(L);
#else
	#error "lua version not found"
#endif

		RegisterAffectFunctionTable();
		RegisterBuildingFunctionTable();
		RegisterDungeonFunctionTable();
		RegisterGameFunctionTable();
		RegisterGuildFunctionTable();
		RegisterHorseFunctionTable();
#ifdef __PET_SYSTEM__
		RegisterPetFunctionTable();
#endif
		RegisterITEMFunctionTable();
		RegisterMarriageFunctionTable();
		RegisterNPCFunctionTable();
		RegisterPartyFunctionTable();
		RegisterPCFunctionTable();
		RegisterQuestFunctionTable();
		RegisterTargetFunctionTable();
		RegisterArenaFunctionTable();
		RegisterForkedFunctionTable();
		RegisterMonarchFunctionTable();
		RegisterOXEventFunctionTable();
		RegisterMgmtFunctionTable();
		RegisterDragonLairFunctionTable();
		RegisterDragonSoulFunctionTable();
#ifdef ENABLE_QUEST_DND_EVENT
		RegisterDNDFunctionTable();
#endif
		{
			luaL_reg member_functions[] =
			{
				{ "chat",			member_chat		},
				{ "set_ready",			member_set_ready	},
				{ "clear_ready",		member_clear_ready	},
				{nullptr, nullptr}
			};

			AddLuaFunctionTable("member", member_functions);
		}

		{
			luaL_reg mob_functions[] =
			{
				{ "spawn",			mob_spawn		},
				{ "spawn_group",		mob_spawn_group		},
				{nullptr, nullptr}
			};

			AddLuaFunctionTable("mob", mob_functions);
		}

		// global namespace functions

		RegisterGlobalFunctionTable(L);

		// LUA_INIT_ERROR_MESSAGE
		{
			char settingsFileName[256];
			snprintf(settingsFileName, sizeof(settingsFileName), "%s/settings.lua", LocaleService_GetBasePath().c_str());

			const int settingsLoadingResult = lua_dofile(L, settingsFileName);
			sys_log(0, "LoadSettings(%s), returns %d", settingsFileName, settingsLoadingResult);
			if (settingsLoadingResult != 0)
			{
				sys_err("LOAD_SETTINS_FAILURE(%s)", settingsFileName);
				return false;
			}
		}

		{
			char questlibFileName[256];
			snprintf(questlibFileName, sizeof(questlibFileName), "%s/questlib.lua", LocaleService_GetQuestPath().c_str());

			const int questlibLoadingResult = lua_dofile(L, questlibFileName);
			sys_log(0, "LoadQuestlib(%s), returns %d", questlibFileName, questlibLoadingResult);
			if (questlibLoadingResult != 0)
			{
				sys_err("LOAD_QUESTLIB_FAILURE(%s)", questlibFileName);
				return false;
			}
		}

#define ENABLE_TRANSLATE_LUA
#ifdef ENABLE_TRANSLATE_LUA
		{
			char translateFileName[256];
			snprintf(translateFileName, sizeof(translateFileName), "%s/translate.lua", LocaleService_GetBasePath().c_str());

			const int translateLoadingResult = lua_dofile(L, translateFileName);
			sys_log(0, "LoadTranslate(%s), returns %d", translateFileName, translateLoadingResult);
			if (translateLoadingResult != 0)
			{
				sys_err("LOAD_TRANSLATE_ERROR(%s)", translateFileName);
				return false;
			}

			// Language System: keep the ES table loaded above under its own
			// global, then load each other language's translate_XX.lua into
			// its own gameforge_XX global. The gameforge proxy installed below
			// resolves gameforge.* reads per player (g_iCurrentLang) with an
			// ES fallback (covers e.g. PT missing the whole change_name block).
			{
				static const int aLangOrder[] =
				{
					LANGUAGE_AE, LANGUAGE_CZ, LANGUAGE_DE, LANGUAGE_DK, LANGUAGE_EN,
					LANGUAGE_FR, LANGUAGE_GR, LANGUAGE_HU, LANGUAGE_IT, LANGUAGE_NL,
					LANGUAGE_PL, LANGUAGE_PT, LANGUAGE_RO, LANGUAGE_RU, LANGUAGE_TR
				};

				lua_getglobal(L, "gameforge");
				lua_setglobal(L, "gameforge_ES");

				char szFileName[256];
				char szGlobalName[32];
				for (size_t i = 0; i < sizeof(aLangOrder) / sizeof(aLangOrder[0]); ++i)
				{
					const int iLang = aLangOrder[i];

					lua_pushnil(L);
					lua_setglobal(L, "gameforge");

					snprintf(szFileName, sizeof(szFileName), "%s/translate_%s.lua", LocaleService_GetBasePath().c_str(), arstLocaleStringNames[iLang].c_str());

					const int langLoadingResult = lua_dofile(L, szFileName);
					sys_log(0, "LoadTranslate(%s), returns %d", szFileName, langLoadingResult);

					if (langLoadingResult == 0)
					{
						lua_getglobal(L, "gameforge");
						snprintf(szGlobalName, sizeof(szGlobalName), "gameforge_%s", arstLocaleStringNames[iLang].c_str());
						lua_setglobal(L, szGlobalName);
					}
					else
					{
						sys_err("LoadTranslate(%s) FAILURE (falls back to ES)", szFileName);
						lua_pop(L, 1); // lua_dofile leaves the error message on the stack
					}
				}

				// gameforge = per-player proxy with ES fallback (ASCII only).
				static const char szGameforgeProxy[] =
					"gameforge = setmetatable({}, {__index=function(t,k)\n"
					"  local g = _G[\"gameforge_\"..string.upper(get_current_lang())]\n"
					"  local v = g and g[k]\n"
					"  if v == nil and _G[\"gameforge_ES\"] then v = _G[\"gameforge_ES\"][k] end\n"
					"  return v\n"
					"end})";

				const int proxyResult = lua_dobuffer(L, szGameforgeProxy, sizeof(szGameforgeProxy) - 1, "gameforge_proxy");
				if (proxyResult != 0)
				{
					sys_err("InstallGameforgeProxy FAILURE (code %d, %s)", proxyResult, lua_tostring(L, -1));
					return false;
				}
			}
		}
#endif

		{
			char questLocaleFileName[256];
			snprintf(questLocaleFileName, sizeof(questLocaleFileName), "%s/locale.lua", g_stQuestDir.c_str());

			const int questLocaleLoadingResult = lua_dofile(L, questLocaleFileName);
			sys_log(0, "LoadQuestLocale(%s), returns %d", questLocaleFileName, questLocaleLoadingResult);
			if (questLocaleLoadingResult != 0)
			{
				/* Non-fatal: quest locale texts are not required for the
				   channel to accept player logins / character select. */
				sys_err("LoadQuestLocale(%s) FAILURE (non-fatal)", questLocaleFileName);
			}
			else
			{
				// Language System: keep the ES mirror loaded above under its
				// own global, then rebuild the mirror for every other language
				// (locale.lua reads through the gameforge proxy, so it bakes
				// the current g_iCurrentLang into the `locale` global) and
				// install a per-player `locale` proxy with ES fallback.
				static const int aLangOrder[] =
				{
					LANGUAGE_AE, LANGUAGE_CZ, LANGUAGE_DE, LANGUAGE_DK, LANGUAGE_EN,
					LANGUAGE_FR, LANGUAGE_GR, LANGUAGE_HU, LANGUAGE_IT, LANGUAGE_NL,
					LANGUAGE_PL, LANGUAGE_PT, LANGUAGE_RO, LANGUAGE_RU, LANGUAGE_TR
				};

				lua_getglobal(L, "locale");
				lua_setglobal(L, "locale_ES");

				const BYTE bSavedLang = g_iCurrentLang;
				char szGlobalName[32];
				for (size_t i = 0; i < sizeof(aLangOrder) / sizeof(aLangOrder[0]); ++i)
				{
					const int iLang = aLangOrder[i];

					g_iCurrentLang = (BYTE) iLang;
					const int langLocaleResult = lua_dofile(L, questLocaleFileName);
					sys_log(0, "LoadQuestLocale(%s) lang %s, returns %d", questLocaleFileName, arstLocaleStringNames[iLang].c_str(), langLocaleResult);

					if (langLocaleResult == 0)
					{
						lua_getglobal(L, "locale");
						snprintf(szGlobalName, sizeof(szGlobalName), "locale_%s", arstLocaleStringNames[iLang].c_str());
						lua_setglobal(L, szGlobalName);
					}
					else
					{
						sys_err("LoadQuestLocale(%s) lang %s FAILURE (non-fatal, falls back to ES)", questLocaleFileName, arstLocaleStringNames[iLang].c_str());
						lua_pop(L, 1); // lua_dofile leaves the error message on the stack
					}
				}
				g_iCurrentLang = bSavedLang;

				// locale = per-player proxy with ES fallback (ASCII only).
				static const char szLocaleProxy[] =
					"locale = setmetatable({}, {__index=function(t,k)\n"
					"  local g = _G[\"locale_\"..string.upper(get_current_lang())]\n"
					"  local v = g and g[k]\n"
					"  if v == nil and _G[\"locale_ES\"] then v = _G[\"locale_ES\"][k] end\n"
					"  return v\n"
					"end})";

				const int proxyResult = lua_dobuffer(L, szLocaleProxy, sizeof(szLocaleProxy) - 1, "locale_proxy");
				if (proxyResult != 0)
					sys_err("InstallLocaleProxy FAILURE (code %d, %s) - locale stays ES", proxyResult, lua_tostring(L, -1));
			}
		}
		// END_OF_LUA_INIT_ERROR_MESSAGE

		for (itertype(g_setQuestObjectDir) it = g_setQuestObjectDir.begin(); it != g_setQuestObjectDir.end(); ++it)
		{
			const string& stQuestObjectDir = *it;
			char buf[PATH_MAX];
			snprintf(buf, sizeof(buf), "%s/state/", stQuestObjectDir.c_str());
			DIR * pdir = opendir(buf);
			int iQuestIdx = 0;

			if (pdir)
			{
				dirent * pde;

				while ((pde = readdir(pdir)))
				{
					if (pde->d_name[0] == '.')
						continue;

					snprintf(buf + 11, sizeof(buf) - 11, "%s", pde->d_name);

					RegisterQuest(pde->d_name, ++iQuestIdx);
					const int ret = lua_dofile(L, (stQuestObjectDir + "/state/" + pde->d_name).c_str());
					sys_log(0, "QUEST: loading %s, returns %d", (stQuestObjectDir + "/state/" + pde->d_name).c_str(), ret);

					BuildStateIndexToName(pde->d_name);
				}

				closedir(pdir);
			}
		}

#if LUA_V == 503
		lua_setgcthreshold(L, 0);
#endif
		lua_newtable(L);
		lua_setglobal(L, "__codecache");
		return true;
	}

	void CQuestManager::GotoSelectState(QuestState& qs)
	{
		lua_checkstack(qs.co, 1);

		//int n = lua_gettop(L);
		const int n = luaL_getn(qs.co, -1);
		qs.args = n;
		//cout << "select here (1-" << qs.args << ")" << endl;

		ostringstream os;
		os << "[QUESTION ";

		for (int i=1; i<=n; i++)
		{
			lua_rawgeti(qs.co,-1,i);
			if (lua_isstring(qs.co,-1))
			{
				//printf("%d\t%s\n",i,lua_tostring(qs.co,-1));
				if (i != 1)
					os << "|";
				os << i << ";" << lua_tostring(qs.co,-1);
			}
			else
			{
				sys_err("SELECT wrong data %s", lua_typename(qs.co, -1));
				sys_err("here");
			}
			lua_pop(qs.co,1);
		}
		os << "]";

		AddScript(os.str());
		qs.suspend_state = SUSPEND_STATE_SELECT;
		if ( test_server )
			sys_log( 0, "%s", m_strScript.c_str() );
		SendScript();
	}

	EVENTINFO(confirm_timeout_event_info)
	{
		DWORD dwWaitPID;
		DWORD dwReplyPID;

		confirm_timeout_event_info()
		: dwWaitPID( 0 )
		, dwReplyPID( 0 )
		{
		}
	};

	EVENTFUNC(confirm_timeout_event)
	{
		const auto info = dynamic_cast<confirm_timeout_event_info *>(event->info);

		if ( info == nullptr)
		{
			sys_err( "confirm_timeout_event> <Factor> Null pointer" );
			return 0;
		}

		const LPCHARACTER chWait = CHARACTER_MANAGER::instance().FindByPID(info->dwWaitPID);
		const LPCHARACTER chReply = nullptr; //CHARACTER_MANAGER::info().FindByPID(info->dwReplyPID);

		if (chReply)
		{
		}

		if (chWait)
		{
			CQuestManager::instance().Confirm(info->dwWaitPID, CONFIRM_TIMEOUT);
		}

		return 0;
	}

	void CQuestManager::GotoConfirmState(QuestState & qs)
	{
		qs.suspend_state = SUSPEND_STATE_CONFIRM;
		const DWORD dwVID = (DWORD) lua_tonumber(qs.co, -3);
		const char* szMsg = lua_tostring(qs.co, -2);
		const int iTimeout = (int) lua_tonumber(qs.co, -1);

		sys_log(0, "GotoConfirmState vid %u msg '%s', timeout %d", dwVID, szMsg, iTimeout);

		// 1
		const LPCHARACTER ch = CHARACTER_MANAGER::instance().Find(dwVID);
		if (ch && ch->IsPC())
		{
			ch->ConfirmWithMsg(szMsg, iTimeout, GetCurrentCharacterPtr()->GetPlayerID());
		}

		// 2
		GetCurrentPC()->SetConfirmWait((ch && ch->IsPC())?ch->GetPlayerID():0);
		ostringstream os;
		os << "[CONFIRM_WAIT timeout;" << iTimeout << "]";
		AddScript(os.str());
		SendScript();

		// 3
		confirm_timeout_event_info* info = AllocEventInfo<confirm_timeout_event_info>();

		info->dwWaitPID = GetCurrentCharacterPtr()->GetPlayerID();
		info->dwReplyPID = (ch && ch->IsPC()) ? ch->GetPlayerID() : 0;

		event_create(confirm_timeout_event, info, PASSES_PER_SEC(iTimeout));
	}

	void CQuestManager::GotoSelectItemState(QuestState& qs)
	{
		qs.suspend_state = SUSPEND_STATE_SELECT_ITEM;
		AddScript("[SELECT_ITEM]");
		SendScript();
	}

	void CQuestManager::GotoInputState(QuestState & qs)
	{
		qs.suspend_state = SUSPEND_STATE_INPUT;
		AddScript("[INPUT]");
		SendScript();
	}

	void CQuestManager::GotoPauseState(QuestState & qs)
	{
		qs.suspend_state = SUSPEND_STATE_PAUSE;
		AddScript("[NEXT]");
		SendScript();
	}

	void CQuestManager::GotoEndState(QuestState & qs)
	{
		AddScript("[DONE]");
		SendScript();
	}

	// * OpenState

	// The beginning of script

	QuestState CQuestManager::OpenState(const string& quest_name, int state_index) const
	{
		QuestState qs;
		qs.args=0;
		qs.st = state_index;
		qs.co = lua_newthread(L);
		qs.ico = lua_ref(L, 1/*qs.co*/);
		return qs;
	}

	// * RunState

	// decides script to wait for user input, or finish

	bool CQuestManager::RunState(QuestState & qs)
	{
		ClearError();

		m_CurrentRunningState = &qs;
		const int ret = lua_resume(qs.co, qs.args);

		if (ret == 0)
		{
			if (lua_gettop(qs.co) == 0)
			{
				// end of quest
				GotoEndState(qs);
				return false;
			}

			if (!strcmp(lua_tostring(qs.co, 1), "select"))
			{
				GotoSelectState(qs);
				return true;
			}

			if (!strcmp(lua_tostring(qs.co, 1), "wait"))
			{
				GotoPauseState(qs);
				return true;
			}

			if (!strcmp(lua_tostring(qs.co, 1), "input"))
			{
				GotoInputState(qs);
				return true;
			}

			if (!strcmp(lua_tostring(qs.co, 1), "confirm"))
			{
				GotoConfirmState(qs);
				return true;
			}

			if (!strcmp(lua_tostring(qs.co, 1), "select_item"))
			{
				GotoSelectItemState(qs);
				return true;
			}
		}
		else
		{
			sys_err("LUA_ERROR: %s", lua_tostring(qs.co, 1));
		}

		WriteRunningStateToSyserr();
		SetError();

		GotoEndState(qs);
		return false;
	}

	// * CloseState

	// makes script end

	void CQuestManager::CloseState(QuestState& qs) const
	{
		if (qs.co)
		{
			//cerr << "ICO "<<qs.ico <<endl;
			lua_unref(L, qs.ico);
			qs.co = nullptr;
		}
	}
}

