#include "StdAfx.h"
#include "AccountConnector.h"
#include "Packet.h"
#include "PythonNetworkStream.h"
#include "PythonLocale.h" // F1: caché del bundle GC_LOCALE
#include "Version.h"
#include "Hwid.h"
#include "../EterBase/tea.h"
#include "../EterPack/EterPackManager.h"

// CHINA_CRYPT_KEY
extern DWORD g_adwEncryptKey[4];
extern DWORD g_adwDecryptKey[4];
// END_OF_CHINA_CRYPT_KEY

void CAccountConnector::SetHandler(PyObject* poHandler)
{
	m_poHandler = poHandler;
}

void CAccountConnector::SetLoginInfo(const char * c_szName, const char * c_szPwd)
{
	m_strID = c_szName;
	m_strPassword = c_szPwd;
}

void CAccountConnector::ClearLoginInfo( void )
{
	m_strPassword = "";
}

bool CAccountConnector::Connect(const char * c_szAddr, int iPort, const char * c_szAccountAddr, int iAccountPort)
{
#if !defined(_IMPROVED_PACKET_ENCRYPTION_) && !defined(USE_NO_PACKET_ENCRYPTION)
	__BuildClientKey();
#endif

	m_strAddr = c_szAddr;
	m_iPort = iPort;

	__OfflineState_Set();

	// CHINA_CRYPT_KEY
	__BuildClientKey_20050304Myevan();
	// END_OF_CHINA_CRYPT_KEY

	return CNetworkStream::Connect(c_szAccountAddr, iAccountPort);
}

void CAccountConnector::Disconnect()
{
	CNetworkStream::Disconnect();
	__OfflineState_Set();
}

void CAccountConnector::Process()
{
	CNetworkStream::Process();

	if (!__StateProcess())
	{
		__OfflineState_Set();
		Disconnect();
	}
}

bool CAccountConnector::__StateProcess()
{
	switch (m_eState)
	{
		case STATE_HANDSHAKE:
			return __HandshakeState_Process();
		case STATE_AUTH:
			return __AuthState_Process();
	}

	return true;
}

bool CAccountConnector::__HandshakeState_Process()
{
	if (!__AnalyzePacket(HEADER_GC_PHASE, sizeof(TPacketGCPhase), &CAccountConnector::__AuthState_RecvPhase))
		return false;

	if (!__AnalyzePacket(HEADER_GC_HANDSHAKE, sizeof(TPacketGCHandshake), &CAccountConnector::__AuthState_RecvHandshake))
		return false;

	if (!__AnalyzePacket(HEADER_GC_PING, sizeof(TPacketGCPing), &CAccountConnector::__AuthState_RecvPing))
		return false;

	// F1: GC_LOCALE (140) puede llegar en cuanto el server recibe el
	// CG_LOCALE_REQUEST (que se envía tras el eco del handshake) — es decir,
	// AÚN en STATE_HANDSHAKE. Var-size con u16 = payload_len (flag + chunk).
	if (!__AnalyzeLocalePacket(&CAccountConnector::__AuthState_RecvLocale))
		return false;

#ifdef _IMPROVED_PACKET_ENCRYPTION_
	if (!__AnalyzePacket(HEADER_GC_KEY_AGREEMENT, sizeof(TPacketKeyAgreement), &CAccountConnector::__AuthState_RecvKeyAgreement))
		return false;

	if (!__AnalyzePacket(HEADER_GC_KEY_AGREEMENT_COMPLETED, sizeof(TPacketKeyAgreementCompleted), &CAccountConnector::__AuthState_RecvKeyAgreementCompleted))
		return false;
#endif

	return true;
}

bool CAccountConnector::__AuthState_Process()
{
	if (!__AnalyzePacket(0, sizeof(BYTE), &CAccountConnector::__AuthState_RecvEmpty))
		return true;

	if (!__AnalyzePacket(HEADER_GC_PHASE, sizeof(TPacketGCPhase), &CAccountConnector::__AuthState_RecvPhase))
		return false;

	if (!__AnalyzePacket(HEADER_GC_PING, sizeof(TPacketGCPing), &CAccountConnector::__AuthState_RecvPing))
		return false;

	// F5: GC_CHANNEL_LIST (164) — lista de canales + manifest (rates) del
	// auth. El auth lo envía ANTES del GC_AUTH_SUCCESS en login exitoso (el
	// cliente consume un paquete por frame y desconecta del auth al despachar
	// el 150 — con 164 después nunca lo leería).
	if (!__AnalyzePacket(HEADER_GC_CHANNEL_LIST, sizeof(TPacketGCChannelList), &CAccountConnector::__AuthState_RecvChannelList))
		return false;

	// F1: GC_LOCALE (140) — bundle de texto del auth (chunked, var-size).
	// Llega tras el CG_LOCALE_REQUEST; si el server lo manda después del
	// GC_AUTH_SUCCESS no se lee (la conexión auth se cierra al despachar el
	// 150) — el server lo envía antes (contrato F1, spec locale-redesign.md).
	if (!__AnalyzeLocalePacket(&CAccountConnector::__AuthState_RecvLocale))
		return false;

	if (!__AnalyzePacket(HEADER_GC_AUTH_SUCCESS, sizeof(TPacketGCAuthSuccess), &CAccountConnector::__AuthState_RecvAuthSuccess))
		return true;

	if (!__AnalyzePacket(HEADER_GC_LOGIN_FAILURE, sizeof(TPacketGCAuthSuccess), &CAccountConnector::__AuthState_RecvAuthFailure))
		return true;

	if (!__AnalyzePacket(HEADER_GC_HANDSHAKE, sizeof(TPacketGCHandshake), &CAccountConnector::__AuthState_RecvHandshake))
		return false;

	if (!__AnalyzePacket(HEADER_GC_PANAMA_PACK, sizeof(TPacketGCPanamaPack), &CAccountConnector::__AuthState_RecvPanamaPack))
		return false;

#ifdef _IMPROVED_PACKET_ENCRYPTION_
	if (!__AnalyzePacket(HEADER_GC_KEY_AGREEMENT, sizeof(TPacketKeyAgreement), &CAccountConnector::__AuthState_RecvKeyAgreement))
		return false;

	if (!__AnalyzePacket(HEADER_GC_KEY_AGREEMENT_COMPLETED, sizeof(TPacketKeyAgreementCompleted), &CAccountConnector::__AuthState_RecvKeyAgreementCompleted))
		return false;
#endif

	return true;
}

bool CAccountConnector::__AuthState_RecvEmpty()
{
	BYTE byEmpty;
	Recv(sizeof(BYTE), &byEmpty);
	return true;
}

bool CAccountConnector::__AuthState_RecvPhase()
{
	TPacketGCPhase kPacketPhase;
	if (!Recv(sizeof(kPacketPhase), &kPacketPhase))
		return false;

	if (kPacketPhase.phase == PHASE_HANDSHAKE)
	{
		__HandshakeState_Set();
	}
	else if (kPacketPhase.phase == PHASE_AUTH)
	{
#if !defined(_IMPROVED_PACKET_ENCRYPTION_) && !defined(USE_NO_PACKET_ENCRYPTION)
		const char* key = LocaleService_GetSecurityKey();
		SetSecurityMode(true, key);
#endif

		TPacketCGLogin3 LoginPacket;
		LoginPacket.header = HEADER_CG_LOGIN3;

		strncpy(LoginPacket.name, m_strID.c_str(), ID_MAX_NUM);
		strncpy(LoginPacket.pwd, m_strPassword.c_str(), PASS_MAX_NUM);
		LoginPacket.name[ID_MAX_NUM] = '\0';
		LoginPacket.pwd[PASS_MAX_NUM] = '\0';

		ClearLoginInfo();
		// NOTE: do NOT clear the CPythonNetworkStream login info here. The
		// channel login (SendLoginPacketNew) still needs m_stPassword; the
		// stream clears it itself right after sending.
		m_strPassword = "";

		for (DWORD i = 0; i < 4; ++i)
			LoginPacket.adwClientKey[i] = g_adwEncryptKey[i];

		// Language System: append client language (2 chars + '\0', e.g. "es")
		const char* szLocale = LocaleService_GetLocaleName();
		LoginPacket.szLanguage[0] = szLocale && szLocale[0] ? szLocale[0] : 'e';
		LoginPacket.szLanguage[1] = szLocale && szLocale[1] ? szLocale[1] : 's';
		LoginPacket.szLanguage[2] = '\0';

		// F2b: AUTH-only extension — client version + 16-byte machine id.
		// The Rust auth requires these 20 bytes (version gate); the legacy
		// C++ auth fallback simply will not serve this client (accepted).
		LoginPacket.dwVersion = (DWORD)METIN2_GET_VERSION();
		GetMachineHwid(LoginPacket.hwid);

		if (!Send(sizeof(LoginPacket), &LoginPacket))
		{
			Tracen(" CAccountConnector::__AuthState_RecvPhase - SendLogin3 Error");
			return false;
		}

		if (!SendSequence())
			return false;

		__AuthState_Set();
	}

	return true;
}

bool CAccountConnector::__AuthState_RecvHandshake()
{
	TPacketGCHandshake kPacketHandshake;
	if (!Recv(sizeof(kPacketHandshake), &kPacketHandshake))
		return false;

	// HandShake
	{
		Tracenf("HANDSHAKE RECV %u %d", kPacketHandshake.dwTime, kPacketHandshake.lDelta);

		ELTimer_SetServerMSec(kPacketHandshake.dwTime+ kPacketHandshake.lDelta);

		//DWORD dwBaseServerTime = kPacketHandshake.dwTime+ kPacketHandshake.lDelta;
		//DWORD dwBaseClientTime = ELTimer_GetMSec();

		kPacketHandshake.dwTime = kPacketHandshake.dwTime + kPacketHandshake.lDelta + kPacketHandshake.lDelta;
		kPacketHandshake.lDelta = 0;

		Tracenf("HANDSHAKE SEND %u", kPacketHandshake.dwTime);

		if (!Send(sizeof(kPacketHandshake), &kPacketHandshake))
		{
			Tracen(" CAccountConnector::__AuthState_RecvHandshake - SendHandshake Error");
			return false;
		}
	}

	// F1 (locale redesign): tras el eco del handshake, pedir el bundle de
	// texto (una vez por conexión — los retries del handshake no lo repiten).
	if (!m_bLocaleRequested)
	{
		if (!SendLocaleRequest())
			return false;
		m_bLocaleRequested = true;
	}

	return true;
}

bool CAccountConnector::SendLocaleRequest()
{
	// CG_LOCALE_REQUEST (132): 0x84 + lang[3] ("es\0") — parity del
	// szLanguage del LOGIN3 (LocaleService_GetLocaleName).
	BYTE buf[4];
	buf[0] = HEADER_CG_LOCALE_REQUEST;
	const char* szLocale = LocaleService_GetLocaleName();
	buf[1] = szLocale && szLocale[0] ? szLocale[0] : 'e';
	buf[2] = szLocale && szLocale[1] ? szLocale[1] : 's';
	buf[3] = '\0';

	CPythonLocale::Instance().SetLanguage(reinterpret_cast<const char*>(buf + 1));

	if (!Send(sizeof(buf), buf))
	{
		Tracen(" CAccountConnector::SendLocaleRequest - Send Error");
		return false;
	}

	Tracenf("CG_LOCALE_REQUEST lang=%c%c", buf[1], buf[2]);
	return true;
}

bool CAccountConnector::__AuthState_RecvLocale(int iTotalSize)
{
	// GC_LOCALE (140): 0x8c + u16 payload_len + u8 chunk_flag + chunk bytes.
	// El u16 del wire es payload_len = flag + chunk (TODO lo que sigue al
	// campo — parity locale.rs `encode_chunks`); el paquete total mide
	// 3 + payload_len. El dispatch garantiza el paquete completo en el
	// buffer (ver __AnalyzeLocalePacket).
	TPacketGCLocale kLocale;
	if (!Recv(sizeof(kLocale), &kLocale))
		return false;

	const int iChunkLen = iTotalSize - 1; // payload_len - flag(1) = chunk
	if (iChunkLen < 0)
		return false;

	if (iChunkLen > 0)
	{
		std::string chunk(iChunkLen, '\0');
		if (!Recv(iChunkLen, &chunk[0]))
			return false;

		if (!CPythonLocale::Instance().AppendChunk(kLocale.chunk_flag, reinterpret_cast<const BYTE*>(chunk.data()), iChunkLen))
		{
			Tracen(" CAccountConnector::__AuthState_RecvLocale - AppendChunk error");
			return false;
		}
	}
	else if (kLocale.chunk_flag == 0)
	{
		// Chunk final vacío: parsea el buffer acumulado (o limpia si no hay).
		CPythonLocale::Instance().AppendChunk(0, nullptr, 0);
	}

	return true;
}

bool CAccountConnector::__AnalyzeLocalePacket(bool (CAccountConnector::*pfnDispatchPacket)(int))
{
	// Igual que __AnalyzeVarSizePacket pero con la semántica del GC_LOCALE:
	// el u16 del wire es payload_len (flag + chunk), NO el tamaño total — el
	// paquete completo mide 3 + payload_len. Se espera el paquete COMPLETO
	// antes de despachar (parity de la garantía anti-race de
	// __AnalyzeVarSizePacket con hybrid-crypt: Recv nunca ve un paquete a
	// medias en el handler).
	BYTE bHeader;
	if (!Peek(sizeof(bHeader), &bHeader))
		return true;

	if (bHeader != HEADER_GC_LOCALE)
		return true;

	TDynamicSizePacketHeader dynamicHeader;
	if (!Peek(sizeof(dynamicHeader), &dynamicHeader))
		return true;

	if (!PeekNoFetch(dynamicHeader.size + sizeof(dynamicHeader)))
		return true;

	return (this->*pfnDispatchPacket)(dynamicHeader.size);
}

bool CAccountConnector::__AuthState_RecvPanamaPack()
{
	TPacketGCPanamaPack kPacket;

	if (!Recv(sizeof(TPacketGCPanamaPack), &kPacket))
		return false;

	CEterPackManager::instance().RegisterPack(kPacket.szPackName, "*", kPacket.abIV);
	return true;
}

bool CAccountConnector::__AuthState_RecvHybridCryptKeys(int iTotalSize)
{
	const int iFixedHeaderSize = TPacketGCHybridCryptKeys::GetFixedHeaderSize();

	TPacketGCHybridCryptKeys kPacket(iTotalSize-iFixedHeaderSize);

	if (!Recv(iFixedHeaderSize, &kPacket))
		return false;

	if (!Recv(kPacket.iKeyStreamLen, kPacket.m_pStream))
		return false;

	CEterPackManager::Instance().RetrieveHybridCryptPackKeys( kPacket.m_pStream );
	return true;
}

bool CAccountConnector::__AuthState_RecvHybridCryptSDB(int iTotalSize)
{
	const int iFixedHeaderSize = TPacketGCHybridSDB::GetFixedHeaderSize();

	TPacketGCHybridSDB kPacket(iTotalSize-iFixedHeaderSize);

	if (!Recv(iFixedHeaderSize, &kPacket))
		return false;

	if (!Recv(kPacket.iSDBStreamLen, kPacket.m_pStream))
		return false;

	CEterPackManager::Instance().RetrieveHybridCryptPackSDB( kPacket.m_pStream );
	return true;
}

bool CAccountConnector::__AuthState_RecvPing()
{
	TPacketGCPing kPacketPing;
	if (!Recv(sizeof(kPacketPing), &kPacketPing))
		return false;

	__AuthState_SendPong();

	return true;
}

bool CAccountConnector::__AuthState_SendPong()
{
	TPacketCGPong kPacketPong;
	kPacketPong.bHeader = HEADER_CG_PONG;
	if (!Send(sizeof(kPacketPong), &kPacketPong))
		return false;

	if (IsSecurityMode())
		return SendSequence();

	return true;
}

bool CAccountConnector::__AuthState_RecvChannelList()
{
	TPacketGCChannelList kPacket;
	if (!Recv(sizeof(kPacket), &kPacket))
		return false;

	// Defensa del contrato de tamaño fijo: count nunca puede exceder el
	// array del struct (un auth malformado no debe leer fuera de límites).
	if (kPacket.count > GC_CHANNEL_LIST_MAX_CHANNELS)
		kPacket.count = GC_CHANNEL_LIST_MAX_CHANNELS;

	m_ChannelList = kPacket;
	m_bHasChannelList = true;

	Tracenf("GC_CHANNEL_LIST: %d canal(es), rates exp %d gold %d drop %d",
		kPacket.count, kPacket.wExpRate, kPacket.wGoldRate, kPacket.wDropRate);
	for (int i = 0; i < kPacket.count; ++i)
		Tracenf("  ch%d: %s %s:%d (%d jugadores)",
			i + 1, kPacket.aChannels[i].szName, kPacket.aChannels[i].szIP,
			kPacket.aChannels[i].wPort, kPacket.aChannels[i].wPlayers);

	return true;
}

bool CAccountConnector::__AuthState_RecvAuthSuccess()
{
	TPacketGCAuthSuccess kAuthSuccessPacket;
	if (!Recv(sizeof(kAuthSuccessPacket), &kAuthSuccessPacket))
		return false;

	if (!kAuthSuccessPacket.bResult)
	{
		if (m_poHandler)
			PyCallClassMemberFunc(m_poHandler, "OnLoginFailure", Py_BuildValue("(s)", "BESAMEKEY"));
	}
	else
	{
		const DWORD dwPanamaKey = kAuthSuccessPacket.dwLoginKey ^ g_adwEncryptKey[0] ^ g_adwEncryptKey[1] ^ g_adwEncryptKey[2] ^ g_adwEncryptKey[3];
		CEterPackManager::instance().DecryptPackIV(dwPanamaKey);

		CPythonNetworkStream & rkNet = CPythonNetworkStream::Instance();
		rkNet.SetLoginKey(kAuthSuccessPacket.dwLoginKey);

		// F5: la lista de canales del auth (GC_CHANNEL_LIST) tiene prioridad —
		// el IP del canal ya NO depende de serverinfo.py. Sin lista (auth C++
		// legacy) → fallback a la dirección bakeada (m_strAddr/m_iPort).
		if (m_bHasChannelList && m_ChannelList.count > 0)
		{
			int iIndex = m_iChannelIndex;
			if (iIndex < 0 || iIndex >= m_ChannelList.count)
				iIndex = 0;

			const TPacketGCChannelListInfo & rkChannel = m_ChannelList.aChannels[iIndex];
			Tracenf("F5: canal %d del auth (%s %s:%d)", iIndex, rkChannel.szName, rkChannel.szIP, rkChannel.wPort);
			rkNet.Connect(rkChannel.szIP, rkChannel.wPort);
		}
		else
		{
			rkNet.Connect(m_strAddr.c_str(), m_iPort);
		}
	}

	Disconnect();
	__OfflineState_Set();

	return true;
}

bool CAccountConnector::__AuthState_RecvAuthFailure()
{
	TPacketGCLoginFailure packet_failure;
	if (!Recv(sizeof(TPacketGCLoginFailure), &packet_failure))
		return false;

	if (m_poHandler)
		PyCallClassMemberFunc(m_poHandler, "OnLoginFailure", Py_BuildValue("(s)", packet_failure.szStatus));

//	__OfflineState_Set();

	return true;
}

#ifdef _IMPROVED_PACKET_ENCRYPTION_
bool CAccountConnector::__AuthState_RecvKeyAgreement()
{
	TPacketKeyAgreement packet;
	if (!Recv(sizeof(packet), &packet))
	{
		return false;
	}

	Tracenf("KEY_AGREEMENT RECV %u", packet.wDataLength);

	TPacketKeyAgreement packetToSend;
	size_t dataLength = TPacketKeyAgreement::MAX_DATA_LEN;
	const size_t agreedLength = Prepare(packetToSend.data, &dataLength);
	if (agreedLength == 0)
	{
		Disconnect();
		return false;
	}
	assert(dataLength <= TPacketKeyAgreement::MAX_DATA_LEN);

	if (Activate(packet.wAgreedLength, packet.data, packet.wDataLength))
	{
		packetToSend.bHeader = HEADER_CG_KEY_AGREEMENT;
		packetToSend.wAgreedLength = (WORD)agreedLength;
		packetToSend.wDataLength = (WORD)dataLength;

		if (!Send(sizeof(packetToSend), &packetToSend))
		{
			Tracen(" CAccountConnector::__AuthState_RecvKeyAgreement - SendKeyAgreement Error");
			return false;
		}
		Tracenf("KEY_AGREEMENT SEND %u", packetToSend.wDataLength);
	}
	else
	{
		Disconnect();
		return false;
	}
	return true;
}

bool CAccountConnector::__AuthState_RecvKeyAgreementCompleted()
{
	TPacketKeyAgreementCompleted packet;
	if (!Recv(sizeof(packet), &packet))
	{
		return false;
	}

	Tracenf("KEY_AGREEMENT_COMPLETED RECV");

	ActivateCipher();

	return true;
}
#endif // _IMPROVED_PACKET_ENCRYPTION_

bool CAccountConnector::__AnalyzePacket(UINT uHeader, UINT uPacketSize, bool (CAccountConnector::*pfnDispatchPacket)())
{
	BYTE bHeader;
	if (!Peek(sizeof(bHeader), &bHeader))
		return true;

	if (bHeader!=uHeader)
		return true;

	if (!PeekNoFetch(uPacketSize))
		return true;

	return (this->*pfnDispatchPacket)();
}

bool CAccountConnector::__AnalyzeVarSizePacket(UINT uHeader, bool (CAccountConnector::*pfnDispatchPacket)(int))
{
	BYTE bHeader;
	if (!Peek(sizeof(bHeader), &bHeader))
		return true;

	if (bHeader!=uHeader)
		return true;

	TDynamicSizePacketHeader dynamicHeader;

	if (!Peek(sizeof(dynamicHeader), &dynamicHeader))
		return true;

	if (!PeekNoFetch(dynamicHeader.size))
		return true;

	return (this->*pfnDispatchPacket)(dynamicHeader.size);
}

void CAccountConnector::__OfflineState_Set()
{
	__Inialize();
}

void CAccountConnector::__HandshakeState_Set()
{
	m_eState=STATE_HANDSHAKE;
}

void CAccountConnector::__AuthState_Set()
{
	m_eState=STATE_AUTH;
}

void CAccountConnector::OnConnectFailure()
{
	if (m_poHandler)
		PyCallClassMemberFunc(m_poHandler, "OnConnectFailure", Py_BuildValue("()"));

	__OfflineState_Set();
}

void CAccountConnector::OnConnectSuccess()
{
	m_eState = STATE_HANDSHAKE;
}

void CAccountConnector::OnRemoteDisconnect()
{
	if (m_isWaitKey)
	{
		if (m_poHandler)
		{
			PyCallClassMemberFunc(m_poHandler, "OnExit", Py_BuildValue("()"));
			return;
		}
	}

	__OfflineState_Set();
}

void CAccountConnector::OnDisconnect()
{
	__OfflineState_Set();
}

#if !defined(_IMPROVED_PACKET_ENCRYPTION_) && !defined(USE_NO_PACKET_ENCRYPTION)
void CAccountConnector::__BuildClientKey()
{
	for (DWORD i = 0; i < 4; ++i)
		g_adwEncryptKey[i] = random();

	const BYTE * c_pszKey = (const BYTE *) "JyTxtHljHJlVJHorRM301vf@4fvj10-v";
	tea_encrypt((DWORD *) g_adwDecryptKey, (const DWORD *) g_adwEncryptKey, (const DWORD *) c_pszKey, 16);
}
#endif

void CAccountConnector::__Inialize()
{
	m_eState=STATE_OFFLINE;
	m_isWaitKey = FALSE;
	m_bLocaleRequested = FALSE; // F1: el bundle se pide una vez por conexión
}

CAccountConnector::CAccountConnector()
{
	m_poHandler = nullptr;
	m_strAddr = "";
	m_iPort = 0;
	m_bHasChannelList = false;
	m_iChannelIndex = -1; // F5: sin selección → primer canal del auth / serverinfo
	m_bLocaleRequested = false;

	SetLoginInfo("", "");
	SetRecvBufferSize(1024 * 128);
	SetSendBufferSize(2048);
	__Inialize();
}

CAccountConnector::~CAccountConnector()
{
	__OfflineState_Set();
}

