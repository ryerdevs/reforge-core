#include "stdafx.h"
#include "config.h"
#include "char.h"
#include "char_manager.h"
#include "db.h"
#include "desc.h"
#include "guild_manager.h"
#include "marriage.h"
#include "../../common/CommonDefines.h"

int CHARACTER::ChangeEmpire(BYTE empire)
{
	if (GetEmpire() == empire)
		return 1;

	char szQuery[1024+1];
	DWORD dwAID;
	DWORD dwPID[PLAYER_PER_ACCOUNT];
	memset(dwPID, 0, sizeof(dwPID));

	{
		snprintf(szQuery, sizeof(szQuery),
				"SELECT id, pid1, pid2, pid3, pid4"
				#ifdef ENABLE_PLAYER_PER_ACCOUNT5
				", pid5"
				#endif
				" FROM player_index%s WHERE pid1=%u OR pid2=%u OR pid3=%u OR pid4=%u"
				#ifdef ENABLE_PLAYER_PER_ACCOUNT5
				" OR pid5=%u"
				#endif
				"AND empire=%u",
				get_table_postfix(), GetPlayerID(), GetPlayerID(), GetPlayerID(), GetPlayerID()
				#ifdef ENABLE_PLAYER_PER_ACCOUNT5
				, GetPlayerID()
				#endif
				, GetEmpire());
		const auto msg(DBManager::instance().DirectQuery(szQuery));
		if (msg->Get()->uiNumRows == 0)
		{
			return 0;
		}

		const MYSQL_ROW row = mysql_fetch_row(msg->Get()->pSQLResult);

		str_to_number(dwAID, row[0]);
		str_to_number(dwPID[0], row[1]);
		str_to_number(dwPID[1], row[2]);
		str_to_number(dwPID[2], row[3]);
		str_to_number(dwPID[3], row[4]);
		#ifdef ENABLE_PLAYER_PER_ACCOUNT5
		str_to_number(dwPID[4], row[5]);
		#endif
	}

	const int loop = PLAYER_PER_ACCOUNT;

	{
		DWORD dwGuildID[PLAYER_PER_ACCOUNT];
		CGuild * pGuild[PLAYER_PER_ACCOUNT];
		std::unique_ptr<SQLMsg> pMsg;

		for (int i = 0; i < loop; ++i)
		{
			snprintf(szQuery, sizeof(szQuery), "SELECT guild_id FROM guild_member%s WHERE pid=%u", get_table_postfix(), dwPID[i]);

			pMsg = DBManager::instance().DirectQuery(szQuery);

			if (pMsg != nullptr)
			{
				if (pMsg->Get()->uiNumRows > 0)
				{
					const MYSQL_ROW row = mysql_fetch_row(pMsg->Get()->pSQLResult);

					str_to_number(dwGuildID[i], row[0]);

					pGuild[i] = CGuildManager::instance().FindGuild(dwGuildID[i]);

					if (pGuild[i] != nullptr)
						return 2;
				}
				else
				{
					dwGuildID[i] = 0;
					pGuild[i] = nullptr;
				}
			}
		}
	}

	{
		for (int i = 0; i < loop; ++i)
		{
			if (marriage::CManager::instance().IsEngagedOrMarried(dwPID[i]) == true)
				return 3;
		}
	}

	{
		snprintf(szQuery, sizeof(szQuery), "UPDATE player_index%s SET empire=%u WHERE pid1=%u OR pid2=%u OR pid3=%u OR pid4=%u"
				#ifdef ENABLE_PLAYER_PER_ACCOUNT5
				" OR pid5=%u"
				#endif
				" AND empire=%u",
				get_table_postfix(), empire, GetPlayerID(), GetPlayerID(), GetPlayerID(), GetPlayerID()
				#ifdef ENABLE_PLAYER_PER_ACCOUNT5
				, GetPlayerID()
				#endif
				, GetEmpire());

		const auto msg(DBManager::instance().DirectQuery(szQuery));
		if (msg->Get()->uiAffectedRows > 0)
		{
			SetChangeEmpireCount();

			return 999;
		}
	}

	return 0;
}

int CHARACTER::GetChangeEmpireCount() const
{
	char szQuery[1024+1];
	const DWORD dwAID = GetAID();

	if (dwAID == 0)
		return 0;

	snprintf(szQuery, sizeof(szQuery), "SELECT change_count FROM change_empire WHERE account_id = %u", dwAID);

	const auto pMsg = DBManager::instance().DirectQuery(szQuery);
	if (pMsg != nullptr)
	{
		if (pMsg->Get()->uiNumRows == 0)
			return 0;

		const MYSQL_ROW row = mysql_fetch_row(pMsg->Get()->pSQLResult);

		DWORD count = 0;
		str_to_number(count, row[0]);
		return count;
	}

	return 0;
}

void CHARACTER::SetChangeEmpireCount() const
{
	char szQuery[1024+1];

	const DWORD dwAID = GetAID();

	if (dwAID == 0) return;

	int count = GetChangeEmpireCount();

	if (count == 0)
	{
		count++;
		snprintf(szQuery, sizeof(szQuery), "INSERT INTO change_empire VALUES(%u, %d, NOW())", dwAID, count);
	}
	else
	{
		count++;
		snprintf(szQuery, sizeof(szQuery), "UPDATE change_empire SET change_count=%d WHERE account_id=%u", count, dwAID);
	}

	auto pmsg(DBManager::instance().DirectQuery(szQuery));
}

DWORD CHARACTER::GetAccountID() const
{
	if (!GetDesc())
		return 0;
	const auto & rkTab = GetDesc()->GetAccountTable();
	return rkTab.id;
}

DWORD CHARACTER::GetAID() const
{
	if (auto accountID = GetAccountID(); accountID) // @fixme318
		return accountID;

	char szQuery[1024+1];
	DWORD dwAID = 0;
	snprintf(szQuery, sizeof(szQuery), "SELECT id FROM player_index%s WHERE pid1=%u OR pid2=%u OR pid3=%u OR pid4=%u"
			#ifdef ENABLE_PLAYER_PER_ACCOUNT5
			" OR pid5=%u"
			#endif
			" AND empire=%u",
			get_table_postfix(), GetPlayerID(), GetPlayerID(), GetPlayerID(), GetPlayerID()
			#ifdef ENABLE_PLAYER_PER_ACCOUNT5
			, GetPlayerID()
			#endif
			, GetEmpire());

	const auto pMsg = DBManager::instance().DirectQuery(szQuery);
	if (pMsg != nullptr)
	{
		if (pMsg->Get()->uiNumRows == 0)
			return 0;

		const MYSQL_ROW row = mysql_fetch_row(pMsg->Get()->pSQLResult);
		str_to_number(dwAID, row[0]);
		return dwAID;
	}
	return 0;
}

