#ifndef __WIN32__
#include <sys/time.h>
#endif

#include <cstdlib>
#include <cstring>
#include <memory>

#ifndef __WIN32__
#include <bsd/string.h>
#endif

#include "AsyncSQL.h"

#ifndef __WIN32__
#define MUTEX_LOCK(mtx) pthread_mutex_lock(mtx)
#define MUTEX_UNLOCK(mtx) pthread_mutex_unlock(mtx)
#else
#define MUTEX_LOCK(mtx) ::EnterCriticalSection(mtx)
#define MUTEX_UNLOCK(mtx) ::LeaveCriticalSection(mtx)
#endif

CAsyncSQL::CAsyncSQL()
	: m_stHost(""), m_stUser(""), m_stPassword(""), m_stDB(""), m_stLocale(""),
	m_iMsgCount(0), m_iPort(0), m_bEnd(false),
#ifndef __WIN32__
	m_hThread(0),
#else
	m_hThread(INVALID_HANDLE_VALUE),
#endif
	m_iQueryFinished(0), m_ulThreadID(0), m_bConnected(false), m_iCopiedQuery(0)
{
	memset( &m_hDB, 0, sizeof(m_hDB) );

	m_aiPipe[0] = 0;
	m_aiPipe[1] = 0;
}

CAsyncSQL::~CAsyncSQL()
{
	Quit();
	Destroy();
}

void CAsyncSQL::Destroy()
{
	if (m_hDB.host)
	{
		sys_log(0, "AsyncSQL: closing mysql connection.");
		mysql_close(&m_hDB);
		m_hDB.host = nullptr;
	}

	if (m_mtxQuery)
	{
#ifndef __WIN32__
		pthread_mutex_destroy(m_mtxQuery.get());
#else
		::DeleteCriticalSection(m_mtxQuery.get());
#endif
		m_mtxQuery.release();
	}

	if (m_mtxResult)
	{
#ifndef __WIN32__
		pthread_mutex_destroy(m_mtxResult.get());
#else
		::DeleteCriticalSection(m_mtxResult.get());
#endif
		m_mtxResult.release();
	}
}

#ifndef __WIN32__
void * AsyncSQLThread(void * arg)
#else
unsigned int __stdcall AsyncSQLThread(void* arg)
#endif
{
	const auto pSQL = ((CAsyncSQL *) arg);

	if (!pSQL->Connect())
		return 0;

	pSQL->ChildLoop();
	return 0;
}

bool CAsyncSQL::QueryLocaleSet()
{
	if (0 == m_stLocale.length())
	{
		sys_err("m_stLocale == 0");
		return true;
	}

	else if (m_stLocale == "ascii")
	{
		sys_err("m_stLocale == ascii");
		return true;
	}

	if (mysql_set_character_set(&m_hDB, m_stLocale.c_str()))
	{
		sys_err("cannot set locale %s by 'mysql_set_character_set', errno %u %s", m_stLocale.c_str(), mysql_errno(&m_hDB) , mysql_error(&m_hDB));
		return false;
	}

	sys_log(0, "\t--mysql_set_character_set(%s)", m_stLocale.c_str());

	return true;
}

bool CAsyncSQL::Connect()
{
	if (nullptr == mysql_init(&m_hDB))
	{
		fprintf(stderr, "mysql_init failed\n");
		return false;
	}

	if (!m_stLocale.empty())
		mysql_options(&m_hDB, MYSQL_SET_CHARSET_NAME, m_stLocale.c_str());

	constexpr bool reconnect = true;
	mysql_options(&m_hDB, MYSQL_OPT_RECONNECT, &reconnect); // @fixme329 must be added before mysql_real_connect

	if (!mysql_real_connect(&m_hDB, m_stHost.c_str(), m_stUser.c_str(), m_stPassword.c_str(), m_stDB.c_str(), m_iPort, nullptr, CLIENT_MULTI_STATEMENTS))
	{
		fprintf(stderr, "mysql_real_connect failed: %s\n", mysql_error(&m_hDB));
		return false;
	}

	#ifdef MARIADB_BASE_VERSION
	int actual_reconnect_val{};
	mysql_get_option(&m_hDB, MYSQL_OPT_RECONNECT, &actual_reconnect_val);
	fprintf(stdout, "AsyncSQL: connected to %s (reconnect %d) [MARIADB]\n", m_stHost.c_str(), actual_reconnect_val);
	#else
	fprintf(stdout, "AsyncSQL: connected to %s (reconnect %d) [MYSQL]\n", m_stHost.c_str(), m_hDB.reconnect);
	#endif

	m_ulThreadID = mysql_thread_id(&m_hDB);
	m_bConnected = true;
	return true;
}

bool CAsyncSQL::Setup(CAsyncSQL * sql, bool bNoThread)
{
	return Setup(sql->m_stHost.c_str(),
			sql->m_stUser.c_str(),
			sql->m_stPassword.c_str(),
			sql->m_stDB.c_str(),
			sql->m_stLocale.c_str(),
			bNoThread,
			sql->m_iPort);
}

bool CAsyncSQL::Setup(const char * c_pszHost, const char * c_pszUser, const char * c_pszPassword, const char * c_pszDB, const char * c_pszLocale, bool bNoThread, int iPort)
{
	m_stHost = c_pszHost;
	m_stUser = c_pszUser;
	m_stPassword = c_pszPassword;
	m_stDB = c_pszDB;
	m_iPort = iPort;

	if (c_pszLocale)
	{
		m_stLocale = c_pszLocale;
		sys_log(0, "AsyncSQL: locale %s", m_stLocale.c_str());
	}

	if (!bNoThread)
	{
#ifndef __WIN32__
		m_mtxQuery = std::make_unique<pthread_mutex_t>();
		m_mtxResult = std::make_unique<pthread_mutex_t>();

		if (0 != pthread_mutex_init(m_mtxQuery.get(), nullptr))
		{
			perror("pthread_mutex_init");
			exit(0);
		}

		if (0 != pthread_mutex_init(m_mtxResult.get(), nullptr))
		{
			perror("pthread_mutex_init");
			exit(0);
		}

		pthread_create(&m_hThread, nullptr, AsyncSQLThread, this);
#else
		m_mtxQuery = std::make_unique<CRITICAL_SECTION>();
		m_mtxResult = std::make_unique<CRITICAL_SECTION>();

		::InitializeCriticalSection(m_mtxQuery.get());
		::InitializeCriticalSection(m_mtxResult.get());

		m_hThread = (HANDLE)::_beginthreadex(nullptr, 0, AsyncSQLThread, this, 0, nullptr);
		if (m_hThread == INVALID_HANDLE_VALUE) {
			perror("CAsyncSQL::Setup");
			return false;
		}
#endif

		return true;
	}
	else
		return Connect();
}

void CAsyncSQL::Quit()
{
	m_bEnd = true;
	m_sem.Release();

#ifndef __WIN32__
	if (m_hThread)
	{
		pthread_join(m_hThread, nullptr);
		m_hThread = 0;
	}
#else
	if (m_hThread != INVALID_HANDLE_VALUE) {
		::WaitForSingleObject(m_hThread, INFINITE);
		m_hThread = INVALID_HANDLE_VALUE;
	}
#endif
}

std::unique_ptr<SQLMsg> CAsyncSQL::DirectQuery(const char * c_pszQuery)
{
	if (m_ulThreadID != mysql_thread_id(&m_hDB))
	{
		sys_log(0, "MySQL connection was reconnected. querying locale set"); // @warme012
		while (!QueryLocaleSet());
		m_ulThreadID = mysql_thread_id(&m_hDB);
	}

	auto p = std::make_unique<SQLMsg>();

	p->m_pkSQL = &m_hDB;
	p->iID = ++m_iMsgCount;
	p->stQuery = c_pszQuery;

	if (mysql_real_query(&m_hDB, p->stQuery.c_str(), p->stQuery.length()))
	{
		char buf[1024];

		snprintf(buf, sizeof(buf),
				"AsyncSQL::DirectQuery : mysql_query error: %s\nquery: %s",
				mysql_error(&m_hDB), p->stQuery.c_str());

		sys_err(buf);
		p->uiSQLErrno = mysql_errno(&m_hDB);
	}

	p->Store();
	return p;
}

void CAsyncSQL::AsyncQuery(const char * c_pszQuery)
{
	const auto p = new SQLMsg;

	p->m_pkSQL = &m_hDB;
	p->iID = ++m_iMsgCount;
	p->stQuery = c_pszQuery;

	PushQuery(p);
}

void CAsyncSQL::ReturnQuery(const char * c_pszQuery, void * pvUserData)
{
	const auto p = new SQLMsg;

	p->m_pkSQL = &m_hDB;
	p->iID = ++m_iMsgCount;
	p->stQuery = c_pszQuery;
	p->bReturn = true;
	p->pvUserData = pvUserData;

	PushQuery(p);
}

void CAsyncSQL::PushResult(SQLMsg * p)
{
	MUTEX_LOCK(m_mtxResult.get());

	m_queue_result.push(p);

	MUTEX_UNLOCK(m_mtxResult.get());
}

bool CAsyncSQL::PopResult(SQLMsg ** pp)
{
	MUTEX_LOCK(m_mtxResult.get());

	if (m_queue_result.empty())
	{
		MUTEX_UNLOCK(m_mtxResult.get());
		return false;
	}

	*pp = m_queue_result.front();
	m_queue_result.pop();
	MUTEX_UNLOCK(m_mtxResult.get());
	return true;
}

void CAsyncSQL::PushQuery(SQLMsg * p)
{
	MUTEX_LOCK(m_mtxQuery.get());

	m_queue_query.push(p);
	//m_map_kSQLMsgUnfinished.insert(std::make_pair(p->iID, p));

	m_sem.Release();

	MUTEX_UNLOCK(m_mtxQuery.get());
}

bool CAsyncSQL::PeekQuery(SQLMsg ** pp) const
{
	MUTEX_LOCK(m_mtxQuery.get());

	if (m_queue_query.empty())
	{
		MUTEX_UNLOCK(m_mtxQuery.get());
		return false;
	}

	*pp = m_queue_query.front();
	MUTEX_UNLOCK(m_mtxQuery.get());
	return true;
}

bool CAsyncSQL::PopQuery(int iID)
{
	MUTEX_LOCK(m_mtxQuery.get());

	if (m_queue_query.empty())
	{
		MUTEX_UNLOCK(m_mtxQuery.get());
		return false;
	}

	m_queue_query.pop();
	//m_map_kSQLMsgUnfinished.erase(iID);

	MUTEX_UNLOCK(m_mtxQuery.get());
	return true;
}

bool CAsyncSQL::PeekQueryFromCopyQueue(SQLMsg ** pp) const
{
	if (m_queue_query_copy.empty())
		return false;

	*pp = m_queue_query_copy.front();
	return true;
}

int CAsyncSQL::CopyQuery()
{
	MUTEX_LOCK(m_mtxQuery.get());

	if (m_queue_query.empty())
	{
		MUTEX_UNLOCK(m_mtxQuery.get());
		return -1;
	}

	while (!m_queue_query.empty())
	{
		SQLMsg * p = m_queue_query.front();
		m_queue_query_copy.push(p);
		m_queue_query.pop();
	}

	//m_map_kSQLMsgUnfinished.erase(iID);

	const int count = m_queue_query_copy.size();

	MUTEX_UNLOCK(m_mtxQuery.get());
	return count;
}

bool CAsyncSQL::PopQueryFromCopyQueue()
{
	if (m_queue_query_copy.empty())
	{
		return false;
	}

	m_queue_query_copy.pop();
	//m_map_kSQLMsgUnfinished.erase(iID);

	return true;
}
int		CAsyncSQL::GetCopiedQueryCount() const
{
	return m_iCopiedQuery;
}
void 	CAsyncSQL::ResetCopiedQueryCount()
{
	m_iCopiedQuery = 0;
}

void	CAsyncSQL::AddCopiedQueryCount(int iCopiedQuery)
{
	m_iCopiedQuery += iCopiedQuery;
}

DWORD CAsyncSQL::CountQuery() const
{
	return m_queue_query.size();
}

DWORD CAsyncSQL::CountResult() const
{
	return m_queue_result.size();
}

void __timediff(struct timeval *a, struct timeval *b, struct timeval *rslt)
{
	if (a->tv_sec < b->tv_sec)
		rslt->tv_sec = rslt->tv_usec = 0;
	else if (a->tv_sec == b->tv_sec)
	{
		if (a->tv_usec < b->tv_usec)
			rslt->tv_sec = rslt->tv_usec = 0;
		else
		{
			rslt->tv_sec = 0;
			rslt->tv_usec = a->tv_usec - b->tv_usec;
		}
	}
	else
	{                      /* a->tv_sec > b->tv_sec */
		rslt->tv_sec = a->tv_sec - b->tv_sec;

		if (a->tv_usec < b->tv_usec)
		{
			rslt->tv_usec = a->tv_usec + 1000000 - b->tv_usec;
			rslt->tv_sec--;
		} else
			rslt->tv_usec = a->tv_usec - b->tv_usec;
	}
}

class cProfiler
{
	public:
		cProfiler()
		{
			m_nInterval = 0 ;

			memset( &prev, 0, sizeof(prev) );
			memset( &now, 0, sizeof(now) );
			memset( &interval, 0, sizeof(interval) );

			Start();
		}

		cProfiler(int nInterval = 100000)
		{
			m_nInterval = nInterval;

			memset( &prev, 0, sizeof(prev) );
			memset( &now, 0, sizeof(now) );
			memset( &interval, 0, sizeof(interval) );

			Start();
		}

		void Start()
		{
			gettimeofday (&prev , (struct timezone *) nullptr);
		}

		void Stop()
		{
			gettimeofday(&now, (struct timezone*) nullptr);
			__timediff(&now, &prev, &interval);
		}

		bool IsOk() const
		{
			if (interval.tv_sec > (m_nInterval / 1000000))
				return false;

			if (interval.tv_usec > m_nInterval)
				return false;

			return true;
		}

		struct timeval * GetResult() { return &interval; }
		long GetResultSec() const { return interval.tv_sec; }
		long GetResultUSec() const { return interval.tv_usec; }

	private:
		int m_nInterval;
		struct timeval  prev;
		struct timeval  now;
		struct timeval	interval;
};

void CAsyncSQL::ChildLoop()
{
	cProfiler profiler(500000);

	while (!m_bEnd)
	{
		m_sem.Wait();

		int count = CopyQuery();

		if (count <= 0)
			continue;

		AddCopiedQueryCount(count);

		SQLMsg * p;

		while (count--)
		{
			profiler.Start();

			if (!PeekQueryFromCopyQueue(&p))
				continue;

			if (m_ulThreadID != mysql_thread_id(&m_hDB))
			{
				sys_log(0, "MySQL connection was reconnected. querying locale set"); // @warme012
				while (!QueryLocaleSet());
				m_ulThreadID = mysql_thread_id(&m_hDB);
			}

			if (mysql_real_query(&m_hDB, p->stQuery.c_str(), p->stQuery.length()))
			{
				p->uiSQLErrno = mysql_errno(&m_hDB);

				sys_err("AsyncSQL: query failed: %s (query: %s errno: %d)",
						mysql_error(&m_hDB), p->stQuery.c_str(), p->uiSQLErrno);

				switch (p->uiSQLErrno)
				{
					case CR_SERVER_LOST: // @fixme329 (errno: 2013)
						if (ResendQuery(p))
							break;
					case CR_SOCKET_CREATE_ERROR:
					case CR_CONNECTION_ERROR:
					case CR_IPSOCK_ERROR:
					case CR_UNKNOWN_HOST:
					case CR_SERVER_GONE_ERROR:
					case CR_CONN_HOST_ERROR:
					case ER_NOT_KEYFILE:
					case ER_CRASHED_ON_USAGE:
					case ER_CANT_OPEN_FILE:
					case ER_HOST_NOT_PRIVILEGED:
					case ER_HOST_IS_BLOCKED:
					case ER_PASSWORD_NOT_ALLOWED:
					case ER_PASSWORD_NO_MATCH:
					case ER_CANT_CREATE_THREAD:
					case ER_INVALID_USE_OF_NULL:
						m_sem.Release();
						sys_err("AsyncSQL: retrying");
						continue;
				}
			}

			profiler.Stop();

			if (!profiler.IsOk())
				sys_log(0, "[QUERY : LONG INTERVAL(OverSec %ld.%ld)] : %s",
						profiler.GetResultSec(), profiler.GetResultUSec(), p->stQuery.c_str());

			PopQueryFromCopyQueue();

			if (p->bReturn)
			{
				p->Store();
				PushResult(p);
			}
			else
				delete p;

			++m_iQueryFinished;
		}
	}

	SQLMsg * p;

	while (PeekQuery(&p))
	{
		if (m_ulThreadID != mysql_thread_id(&m_hDB))
		{
			sys_log(0, "MySQL connection was reconnected. querying locale set"); // @warme012
			while (!QueryLocaleSet());
			m_ulThreadID = mysql_thread_id(&m_hDB);
		}

		if (mysql_real_query(&m_hDB, p->stQuery.c_str(), p->stQuery.length()))
		{
			p->uiSQLErrno = mysql_errno(&m_hDB);

			sys_err("AsyncSQL::ChildLoop : mysql_query error: %s:\nquery: %s",
					mysql_error(&m_hDB), p->stQuery.c_str());

			switch (p->uiSQLErrno)
			{
				case CR_SERVER_LOST: // @fixme329 (errno: 2013)
					if (ResendQuery(p))
						break;
				case CR_SOCKET_CREATE_ERROR:
				case CR_CONNECTION_ERROR:
				case CR_IPSOCK_ERROR:
				case CR_UNKNOWN_HOST:
				case CR_SERVER_GONE_ERROR:
				case CR_CONN_HOST_ERROR:
				case ER_NOT_KEYFILE:
				case ER_CRASHED_ON_USAGE:
				case ER_CANT_OPEN_FILE:
				case ER_HOST_NOT_PRIVILEGED:
				case ER_HOST_IS_BLOCKED:
				case ER_PASSWORD_NOT_ALLOWED:
				case ER_PASSWORD_NO_MATCH:
				case ER_CANT_CREATE_THREAD:
				case ER_INVALID_USE_OF_NULL:
					continue;
			}
		}

		sys_log(0, "QUERY_FLUSH: %s", p->stQuery.c_str());

		PopQuery(p->iID);

		if (p->bReturn)
		{
			p->Store();
			PushResult(p);
		}
		else
			delete p;

		++m_iQueryFinished;
	}
}

int CAsyncSQL::CountQueryFinished() const
{
	return m_iQueryFinished;
}

void CAsyncSQL::ResetQueryFinished()
{
	m_iQueryFinished = 0;
}

MYSQL * CAsyncSQL::GetSQLHandle()
{
	return &m_hDB;
}

size_t CAsyncSQL::EscapeString(char* dst, size_t dstSize, const char *src, size_t srcSize)
{
	if (0 == srcSize)
	{
		memset(dst, 0, dstSize);
		return 0;
	}

	if (0 == dstSize)
		return 0;

	if (dstSize < srcSize * 2 + 1)
	{
		char tmp[256];
		const size_t tmpLen = sizeof(tmp) > srcSize ? srcSize : sizeof(tmp);
		strlcpy(tmp, src, tmpLen);

		sys_err("FATAL ERROR!! not enough buffer size (dstSize %u srcSize %u src%s: %s)",
				dstSize, srcSize, tmpLen != srcSize ? "(trimmed to 255 characters)" : "", tmp);

		dst[0] = '\0';
		return 0;
	}

	return mysql_real_escape_string(GetSQLHandle(), dst, src, srcSize);
}

void CAsyncSQL2::SetLocale(const std::string & stLocale)
{
	m_stLocale = stLocale;
	QueryLocaleSet();
}

bool CAsyncSQL::ResendQuery(SQLMsg * p)
{
#ifdef MARIADB_BASE_VERSION
	if (mariadb_reconnect(&m_hDB) != 0)
	{
		sys_err("MariaDB reconnection failed: %s", mysql_error(&m_hDB));
		return false;
	}
	else
		sys_err("MariaDB reconnection successful.");
#else
	#ifndef CR_ALREADY_CONNECTED
	#define CR_ALREADY_CONNECTED 2058
	#endif
	if (!mysql_real_connect(&m_hDB, m_stHost.c_str(), m_stUser.c_str(), m_stPassword.c_str(), m_stDB.c_str(), m_iPort, nullptr, CLIENT_MULTI_STATEMENTS))
	{
		p->uiSQLErrno = mysql_errno(&m_hDB);
		if (p->uiSQLErrno != CR_ALREADY_CONNECTED)
		{
			sys_err("MySQL reconnection failed: %s", mysql_error(&m_hDB));
			return false;
		}
		else
			sys_err("MySQL already reconnected successful.");
	}
	else
		sys_err("MySQL reconnection successful.");
#endif
	// reset locale set for integrity
	if (m_ulThreadID != mysql_thread_id(&m_hDB))
	{
		sys_log(0, "MySQL connection was reconnected. querying locale set"); // @warme012
		while (!QueryLocaleSet());
		m_ulThreadID = mysql_thread_id(&m_hDB);
	}

	// resend the query after reconnecting
	return mysql_real_query(&m_hDB, p->stQuery.c_str(), p->stQuery.length()) == 0;
}

