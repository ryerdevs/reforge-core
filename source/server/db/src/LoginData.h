#ifndef __INC_METIN_II_DB_LOGINDATA_H__
#define __INC_METIN_II_DB_LOGINDATA_H__

class CLoginData
{
    public:
	CLoginData();

	TAccountTable & GetAccountRef();
	void            SetClientKey(const DWORD * c_pdwClientKey);

	const DWORD *   GetClientKey() const;
	void            SetKey(DWORD dwKey);
	DWORD           GetKey() const;

	void            SetConnectedPeerHandle(DWORD dwHandle);
	DWORD		GetConnectedPeerHandle() const;

	void            SetLogonTime();
	DWORD		GetLogonTime() const;

	void		SetIP(const char * c_pszIP);
	const char *	GetIP() const;

	void		SetPlay(bool bOn);
	bool		IsPlay() const;

	void		SetDeleted(bool bSet);
	bool		IsDeleted() const;

	time_t		GetLastPlayTime() const { return m_lastPlayTime; }

	void            SetPremium(int * paiPremiumTimes);
	int             GetPremium(BYTE type) const;
	int *           GetPremiumPtr();

	DWORD		GetLastPlayerID() const { return m_dwLastPlayerID; }
	void		SetLastPlayerID(DWORD id) { m_dwLastPlayerID = id; }

    private:
	DWORD           m_dwKey;
	DWORD           m_adwClientKey[4];
	DWORD           m_dwConnectedPeerHandle;
	DWORD           m_dwLogonTime;
	char		m_szIP[MAX_HOST_LENGTH+1];
	bool		m_bPlay;
	bool		m_bDeleted;

	time_t		m_lastPlayTime;
	int		m_aiPremiumTimes[PREMIUM_MAX_NUM];

	DWORD		m_dwLastPlayerID;

	TAccountTable   m_data;

// @fixme353 BEGIN
public:
	bool IsAllowLoginByKey() const noexcept { return m_bAllowLoginByKey; }
	void SetAllowLoginByKey(bool bFlag) noexcept { m_bAllowLoginByKey = bFlag; }

private:
	bool m_bAllowLoginByKey{true};
// @fixme353 END
};

#endif

