#ifndef __INC_METIN_II_GAME_DESC_H__
#define __INC_METIN_II_GAME_DESC_H__

#include "../../common/stl.h"
#include "constants.h"
#include "input.h"
#ifdef USE_IMPROVED_PACKET_DECRYPTED_BUFFER
#include "buffer_manager.h"
#endif
#ifdef _IMPROVED_PACKET_ENCRYPTION_
#include "cipher.h"
#endif

#define MAX_ALLOW_USER                  4096
#define MAX_INPUT_LEN			65536

#define HANDSHAKE_RETRY_LIMIT		32

class CInputProcessor;

enum EDescType
{
	DESC_TYPE_ACCEPTOR,
	DESC_TYPE_CONNECTOR
};

class CLoginKey
{
	public:
		CLoginKey(DWORD dwKey, LPDESC pkDesc) : m_dwKey(dwKey), m_pkDesc(pkDesc)
		{
			m_dwExpireTime = 0;
		}

		void Expire()
		{
			m_dwExpireTime = get_dword_time();
			m_pkDesc = nullptr;
		}

		operator DWORD() const
		{
			return m_dwKey;
		}

		DWORD   m_dwKey;
		DWORD   m_dwExpireTime;
		LPDESC  m_pkDesc;
};

#ifdef ENABLE_SEQUENCE_SYSTEM

struct seq_t
{
	BYTE	hdr;
	BYTE	seq;
};
typedef std::vector<seq_t>	seq_vector_t;

#endif

class DESC
{
	public:
		EVENTINFO(desc_event_info)
		{
			LPDESC desc;

			desc_event_info()
			: desc(nullptr)
			{
			}
		};

	public:
		DESC();
		virtual ~DESC();

		virtual BYTE		GetType() { return DESC_TYPE_ACCEPTOR; }
		virtual void		Destroy();
		virtual void		SetPhase(int _phase);

		void			FlushOutput();

		bool			Setup(LPFDWATCH _fdw, socket_t _fd, const struct sockaddr_in & c_rSockAddr, DWORD _handle, DWORD _handshake);

		socket_t		GetSocket() const	{ return m_sock; }
		const char *	GetHostName() const { return m_stHost.c_str(); }
		WORD			GetPort() const { return m_wPort; }

		void			SetP2P(const char * h, WORD w, BYTE b) { m_stP2PHost = h; m_wP2PPort = w; m_bP2PChannel = b; }
		const char *	GetP2PHost() const { return m_stP2PHost.c_str();	}
		WORD			GetP2PPort() const		{ return m_wP2PPort; }
		BYTE			GetP2PChannel() const	{ return m_bP2PChannel;	}

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void BufferedPacket(const T& c_pvData) {
			BufferedPacket(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void BufferedPacket(const C& v) {
			BufferedPacket(v.data(), v.size() * sizeof(typename C::value_type));
		}

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void Packet(const T& c_pvData) {
			Packet(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void Packet(const C& v) {
			Packet(v.data(), v.size() * sizeof(typename C::value_type));
		}

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void LargePacket(const T& c_pvData) {
			LargePacket(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void LargePacket(const C& v) {
			LargePacket(v.data(), v.size() * sizeof(typename C::value_type));
		}

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void RawPacket(const T& c_pvData) {
			RawPacket(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void RawPacket(const C& v) {
			RawPacket(v.data(), v.size() * sizeof(typename C::value_type));
		}

		void			BufferedPacket(const void * c_pvData, int iSize);
		void			Packet(const void * c_pvData, int iSize);
		void			LargePacket(const void * c_pvData, int iSize);

		int			ProcessInput();		// returns -1 if error
		int			ProcessOutput();	// returns -1 if error

		CInputProcessor	*	GetInputProcessor() const { return m_pInputProcessor; }

		DWORD			GetHandle() const	{ return m_dwHandle; }
		LPBUFFER		GetOutputBuffer() const { return m_lpOutputBuffer; }

		void			BindAccountTable(TAccountTable * pTable);
		TAccountTable &		GetAccountTable()	{ return m_accountTable; }

		void			BindCharacter(LPCHARACTER ch);
		LPCHARACTER		GetCharacter() const { return m_lpCharacter; }

		bool			IsPhase(int phase) const	{ return m_iPhase == phase ? true : false; }

		const struct sockaddr_in & GetAddr() const { return m_SockAddr;	}

		void			Log(const char * format, ...) const;

		void			StartHandshake(DWORD _dw);
		void			SendHandshake(DWORD dwCurTime, long lNewDelta);
		bool			HandshakeProcess(DWORD dwTime, long lDelta, bool bInfiniteRetry=false);
		bool			IsHandshaking() const;

		DWORD			GetHandshake() const	{ return m_dwHandshake; }
		DWORD			GetClientTime() const;

#ifdef _IMPROVED_PACKET_ENCRYPTION_
		void			SendKeyAgreement();
		void			SendKeyAgreementCompleted();
		bool			FinishHandshake(size_t agreed_length, const void* buffer, size_t length);
		bool			IsCipherPrepared();
#elif !defined(USE_NO_PACKET_ENCRYPTION)
		// Obsolete encryption stuff here
		void			SetSecurityKey(const DWORD * c_pdwKey);
		const DWORD *	GetEncryptionKey() const { return &m_adwEncryptionKey[0]; }
		const DWORD *	GetDecryptionKey() const { return &m_adwDecryptionKey[0]; }
#endif

		BYTE			GetEmpire() const;

#ifdef __LANGUAGE_SYSTEM__
		// Language System: account language (LANGUAGE_* enum from locale.hpp)
		BYTE			GetLang() const { return m_bLang; }
		void			SetLang(BYTE bLang) { m_bLang = bLang; }
#endif

		// for p2p
		void			SetRelay(const char * c_pszName);
		bool			DelayedDisconnect(int iSec);
		void			DisconnectOfSameLogin();

		void			SetAdminMode();
		bool			IsAdminMode() const;

		void			SetPong(bool b);
		bool			IsPong() const;

#ifdef ENABLE_SEQUENCE_SYSTEM
		BYTE			GetSequence();
		void			SetNextSequence();
#endif

		void			SendLoginSuccessPacket();

		void			SetPanamaKey(DWORD dwKey)	{m_dwPanamaKey = dwKey;}
		DWORD			GetPanamaKey() const		{ return m_dwPanamaKey; }

		void			SetLoginKey(DWORD dwKey);
		void			SetLoginKey(CLoginKey * pkKey);
		DWORD			GetLoginKey() const;

		void			AssembleCRCMagicCube(BYTE bProcPiece, BYTE bFilePiece);

		void			SetClientVersion(const char * c_pszTimestamp) { m_stClientVersion = c_pszTimestamp; }
		const char *		GetClientVersion() const { return m_stClientVersion.c_str(); }

		bool			isChannelStatusRequested() const { return m_bChannelStatusRequested; }
		void			SetChannelStatusRequested(bool bChannelStatusRequested) { m_bChannelStatusRequested = bChannelStatusRequested; }

	protected:
		void			Initialize();

	protected:
		CInputProcessor *	m_pInputProcessor;
		CInputClose		m_inputClose;
		CInputHandshake	m_inputHandshake;
		CInputLogin		m_inputLogin;
		CInputMain		m_inputMain;
		CInputDead		m_inputDead;
		CInputAuth		m_inputAuth;

		LPFDWATCH		m_lpFdw;
		socket_t		m_sock;
		int				m_iPhase;
		DWORD			m_dwHandle;

		std::string		m_stHost;
		WORD			m_wPort;
		time_t			m_LastTryToConnectTime;

		LPBUFFER		m_lpInputBuffer;
#ifdef USE_IMPROVED_PACKET_DECRYPTED_BUFFER
		TEMP_BUFFER		m_lpInputDecryptedBuffer;
#endif
		int				m_iMinInputBufferLen;

		DWORD			m_dwHandshake;
		DWORD			m_dwHandshakeSentTime;
		int				m_iHandshakeRetry;
		DWORD			m_dwClientTime;
		bool			m_bHandshaking;

		LPBUFFER		m_lpBufferedOutputBuffer;
		LPBUFFER		m_lpOutputBuffer;

		LPEVENT			m_pkPingEvent;
		LPCHARACTER		m_lpCharacter;
		TAccountTable		m_accountTable;

		struct sockaddr_in	m_SockAddr;

		FILE *			m_pLogFile;
		std::string		m_stRelayName;

		std::string		m_stP2PHost;
		WORD			m_wP2PPort;
		BYTE			m_bP2PChannel;

#ifdef __LANGUAGE_SYSTEM__
		BYTE			m_bLang;	// Language System: per-account language
#endif

		bool			m_bAdminMode;
		bool			m_bPong;

#ifdef ENABLE_SEQUENCE_SYSTEM
		int			m_iCurrentSequence;
#endif

		CLoginKey *		m_pkLoginKey;
		DWORD			m_dwLoginKey;
		DWORD			m_dwPanamaKey;

		BYTE                    m_bCRCMagicCubeIdx;
		DWORD                   m_dwProcCRC;
		DWORD                   m_dwFileCRC;
		bool			m_bHackCRCQuery;

		std::string		m_stClientVersion;

		std::string		m_Login;
		int				m_outtime;
		int				m_playtime;
		int				m_offtime;

		bool			m_bDestroyed;
		bool			m_bChannelStatusRequested;

#ifdef _IMPROVED_PACKET_ENCRYPTION_
		Cipher cipher_;
#elif !defined(USE_NO_PACKET_ENCRYPTION)
		// Obsolete encryption stuff here
		bool			m_bEncrypted;
		DWORD			m_adwDecryptionKey[4];
		DWORD			m_adwEncryptionKey[4];
#endif

	public:
		LPEVENT			m_pkDisconnectEvent;

	public:
		void SetLogin( const std::string & login ) { m_Login = login; }
		void SetLogin( const char * login ) { m_Login = login; }
		const std::string& GetLogin() { return m_Login; }

		void SetOutTime( int outtime ) { m_outtime = outtime; }
		void SetOffTime( int offtime ) { m_offtime = offtime; }
		void SetPlayTime( int playtime ) { m_playtime = playtime; }

		void RawPacket(const void * c_pvData, int iSize);
		void ChatPacket(BYTE type, const char * format, ...);

#ifdef ENABLE_SEQUENCE_SYSTEM

	public:
		seq_vector_t	m_seq_vector;
		void			push_seq (BYTE hdr, BYTE seq);
#endif
};

#endif

