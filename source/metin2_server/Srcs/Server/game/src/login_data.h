#ifndef __INC_METIN_II_LOGIN_DATA_H__
#define __INC_METIN_II_LOGIN_DATA_H__

class CLoginData
{
	public:
		CLoginData();

		void            SetClientKey(const DWORD * c_pdwClientKey);

		const DWORD *   GetClientKey() const;
		void            SetKey(DWORD dwKey);
		DWORD           GetKey() const;

		void			SetLogin(const char * c_pszLogin);
		const char *	GetLogin() const;

		void			SetConnectedPeerHandle(DWORD dwHandle);
		DWORD			GetConnectedPeerHandle() const;

		void			SetLogonTime();
		DWORD			GetLogonTime() const;

		void			SetIP(const char * c_pszIP);
		const char *	GetIP() const;

		void		SetDeleted(bool bSet);
		bool		IsDeleted() const;

		void		SetPremium(int * paiPremiumTimes);
		int			GetPremium(BYTE type) const;
		int *		GetPremiumPtr();

	private:
		DWORD           m_dwKey;
		DWORD           m_adwClientKey[4];
		DWORD           m_dwConnectedPeerHandle;
		DWORD           m_dwLogonTime;
		char			m_szIP[MAX_HOST_LENGTH+1];
		bool			m_bDeleted;
		std::string		m_stLogin;
		int				m_aiPremiumTimes[PREMIUM_MAX_NUM];
};

#endif

