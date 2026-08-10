#ifndef __INC_METIN2_DB_DBMANAGER_H__
#define __INC_METIN2_DB_DBMANAGER_H__

#include <mysql/mysql.h>

#include "../../libsql/AsyncSQL.h"
#include <memory>

#define SQL_SAFE_LENGTH(size)	(size * 2 + 1)
#define QUERY_SAFE_LENGTH(size)	(1024 + SQL_SAFE_LENGTH(size))

class CQueryInfo
{
    public:
	int	iType;
	DWORD	dwIdent;
	void *	pvData;
};

enum eSQL_SLOT
{
    SQL_PLAYER,
    SQL_ACCOUNT,
	SQL_COMMON,
#ifdef ENABLE_DB_SQL_LOG
	SQL_LOG,
#endif
    SQL_MAX_NUM,
};

class CDBManager : public singleton<CDBManager>
{
    protected:
	void			Initialize();
	void			Destroy();

    public:
	CDBManager();
	virtual ~CDBManager();

	void			Clear();
	void			Quit() const;

	int			Connect(int iSlot, const char * host, int port, const char* dbname, const char* user, const char* pass);

	void			ReturnQuery(const char * c_pszQuery, int iType, DWORD dwIdent, void * pvData, int iSlot = SQL_PLAYER) const;
	void			AsyncQuery(const char * c_pszQuery, int iSlot = SQL_PLAYER) const;
	std::unique_ptr<SQLMsg>		DirectQuery(const char * c_pszQuery, int iSlot = SQL_PLAYER) const;

	SQLMsg *		PopResult() const;
	SQLMsg * 		PopResult(eSQL_SLOT slot ) const;

	unsigned long		EscapeString(void * to, const void * from, unsigned long length, int iSlot = SQL_PLAYER) const;

	DWORD			CountReturnQuery(int i) const { return m_mainSQL[i] ? m_mainSQL[i]->CountQuery() : 0; }
	DWORD			CountReturnResult(int i) const { return m_mainSQL[i] ? m_mainSQL[i]->CountResult() : 0; }
	DWORD			CountReturnQueryFinished(int i) const { return m_mainSQL[i] ? m_mainSQL[i]->CountQueryFinished() : 0; }
	DWORD			CountReturnCopiedQuery(int i) const { return m_mainSQL[i] ? m_mainSQL[i]->GetCopiedQueryCount() : 0; }

	DWORD			CountAsyncQuery(int i) const { return m_asyncSQL[i] ? m_asyncSQL[i]->CountQuery() : 0; }
	DWORD			CountAsyncResult(int i) const { return m_asyncSQL[i] ? m_asyncSQL[i]->CountResult() : 0; }
	DWORD			CountAsyncQueryFinished(int i) const { return m_asyncSQL[i] ? m_asyncSQL[i]->CountQueryFinished() : 0; }
	DWORD			CountAsyncCopiedQuery(int i) const { return m_asyncSQL[i] ? m_asyncSQL[i]->GetCopiedQueryCount() : 0; }

	void			ResetCounter() const
	{
	    for (int i = 0; i < SQL_MAX_NUM; ++i)
		{
			if (m_mainSQL[i])
			{
				m_mainSQL[i]->ResetQueryFinished();
				m_mainSQL[i]->ResetCopiedQueryCount();
			}

			if (m_asyncSQL[i])
			{
				m_asyncSQL[i]->ResetQueryFinished();
				m_asyncSQL[i]->ResetCopiedQueryCount();
			}
		}
	}

    private:
	std::unique_ptr<CAsyncSQL2>		m_mainSQL[SQL_MAX_NUM];
	std::unique_ptr<CAsyncSQL2>	 	m_directSQL[SQL_MAX_NUM];
	std::unique_ptr<CAsyncSQL2>		m_asyncSQL[SQL_MAX_NUM];

	//CHARSET
	public:
	void SetLocale(const char * szLocale ) const;
	void QueryLocaleSet() const;
	private:

	//END_CHARSET
};

#endif

