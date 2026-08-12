#pragma once

#include "../EterLib/NetStream.h"
#include "../EterLib/FuncObject.h"
#include "Packet.h" // F5: TPacketGCChannelList (GC_CHANNEL_LIST 164)

class CAccountConnector : public CNetworkStream, public CSingleton<CAccountConnector>
{
	public:
		enum
		{
			STATE_OFFLINE,
			STATE_HANDSHAKE,
			STATE_AUTH,
		};

	public:
		CAccountConnector();
		virtual ~CAccountConnector();

		void SetHandler(PyObject* poHandler);
		void SetLoginInfo(const char * c_szName, const char * c_szPwd);
		void ClearLoginInfo( void );

		bool Connect(const char * c_szAddr, int iPort, const char * c_szAccountAddr, int iAccountPort);
		void Disconnect();
		void Process();

		// F1 (locale redesign): pide el bundle de texto al auth
		// (CG_LOCALE_REQUEST 132 → GC_LOCALE 140). Se envía tras el eco del
		// handshake; re-request futuro para hot reload.
		bool SendLocaleRequest();

		// F5 — lista de canales + manifest (rates) del auth (GC_CHANNEL_LIST).
		// El índice del canal elegido en la UI (0-based, la key del dict de
		// intrologin.py) con el que RecvAuthSuccess conecta al canal.
		void SetChannelIndex(int iChannelIndex) { m_iChannelIndex = iChannelIndex; }
		bool HasChannelList() const { return m_bHasChannelList; }
		BYTE GetChannelCount() const { return m_ChannelList.count; }
		const TPacketGCChannelListInfo & GetChannel(int i) const { return m_ChannelList.aChannels[i]; }
		WORD GetExpRate() const { return m_ChannelList.wExpRate; }
		WORD GetGoldRate() const { return m_ChannelList.wGoldRate; }
		WORD GetDropRate() const { return m_ChannelList.wDropRate; }

	protected:
		void OnConnectFailure();
		void OnConnectSuccess();
		void OnRemoteDisconnect();
		void OnDisconnect();

	protected:
		void __Inialize();
		bool __StateProcess();

		void __OfflineState_Set();
		void __HandshakeState_Set();
		void __AuthState_Set();

		bool __HandshakeState_Process();
		bool __AuthState_Process();

		bool __AuthState_RecvEmpty();
		bool __AuthState_RecvPhase();
		bool __AuthState_RecvHandshake();
		bool __AuthState_RecvPing();
		bool __AuthState_SendPong();
		bool __AuthState_RecvAuthSuccess();
		bool __AuthState_RecvAuthFailure();
		bool __AuthState_RecvPanamaPack();
		bool __AuthState_RecvChannelList(); // F5: GC_CHANNEL_LIST (164)
		bool __AuthState_RecvLocale(int iTotalSize); // F1: GC_LOCALE (140, var-size)

		bool __AnalyzeLocalePacket(bool (CAccountConnector::*pfnDispatchPacket)(int)); // F1
#ifdef _IMPROVED_PACKET_ENCRYPTION_
		bool __AuthState_RecvKeyAgreement();
		bool __AuthState_RecvKeyAgreementCompleted();
#endif
		bool __AuthState_RecvHybridCryptKeys(int VarSize);
		bool __AuthState_RecvHybridCryptSDB(int VarSize);

		bool __AnalyzePacket(UINT uHeader, UINT uPacketSize, bool (CAccountConnector::*pfnDispatchPacket)());
		bool __AnalyzeVarSizePacket(UINT uHeader, bool (CAccountConnector::*pfnDispatchPacket)(int));

#if !defined(_IMPROVED_PACKET_ENCRYPTION_) && !defined(USE_NO_PACKET_ENCRYPTION)
		void __BuildClientKey();
#endif

	protected:
		UINT m_eState;
		std::string m_strID;
		std::string m_strPassword;

		std::string m_strAddr;
		int m_iPort;
		BOOL m_isWaitKey;

		PyObject * m_poHandler;

		// F5 — lista de canales + manifest del auth. NO se limpia en
		// __Inialize(): es dato de sesión para intrologin.py (la UI del
		// selector usa net.GetChannelList() hasta el próximo login).
		TPacketGCChannelList m_ChannelList;
		bool m_bHasChannelList;
		int m_iChannelIndex;

		// F1 — locale bundle: el request se envía una vez por conexión
		// (tras el eco del handshake; los retries del handshake no lo repiten).
		bool m_bLocaleRequested;

		// CHINA_CRYPT_KEY
		void __BuildClientKey_20050304Myevan() const;
		// END_OF_CHINA_CRYPT_KEY
};

