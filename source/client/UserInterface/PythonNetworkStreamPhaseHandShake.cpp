#include "StdAfx.h"
#include "PythonNetworkStream.h"
#include "PythonApplication.h"
#include "Packet.h"
#include "../eterpack/EterPackManager.h"

// HandShake ---------------------------------------------------------------------------
void CPythonNetworkStream::HandShakePhase()
{
	TPacketHeader header;

	if (!CheckPacket(&header))
		return;

#if defined(_DEBUG) && defined(ENABLE_PRINT_RECV_PACKET_DEBUG)
	Tracenf("RECV HEADER : %u , phase %s ", header, m_strPhase.c_str());
#endif

	switch (header)
	{
		case HEADER_GC_PHASE:
			if (RecvPhasePacket())
				return;
			break;

		case HEADER_GC_HANDSHAKE:
			{
				if (!Recv(sizeof(TPacketGCHandshake), &m_HandshakeData))
					return;

				Tracenf("HANDSHAKE RECV %u %d", m_HandshakeData.dwTime, m_HandshakeData.lDelta);

				ELTimer_SetServerMSec(m_HandshakeData.dwTime+ m_HandshakeData.lDelta);

				//m_dwBaseServerTime = m_HandshakeData.dwTime+ m_HandshakeData.lDelta;
				//m_dwBaseClientTime = ELTimer_GetMSec();

				m_HandshakeData.dwTime = m_HandshakeData.dwTime + m_HandshakeData.lDelta + m_HandshakeData.lDelta;
				m_HandshakeData.lDelta = 0;

				Tracenf("HANDSHAKE SEND %u", m_HandshakeData.dwTime);

				if (!Send(sizeof(TPacketGCHandshake), &m_HandshakeData))
				{
					assert(!"Failed Sending Handshake");
					return;
				}

				CTimer::Instance().SetBaseTime();
				return;
			}
		case HEADER_GC_PING:
			RecvPingPacket();
			return;

		case HEADER_GC_HYBRIDCRYPT_KEYS:
			RecvHybridCryptKeyPacket();
			return;

		case HEADER_GC_HYBRIDCRYPT_SDB:
			RecvHybridCryptSDBPacket();
			return;

#ifdef _IMPROVED_PACKET_ENCRYPTION_
		case HEADER_GC_KEY_AGREEMENT:
			RecvKeyAgreementPacket();
			return;

		case HEADER_GC_KEY_AGREEMENT_COMPLETED:
			RecvKeyAgreementCompletedPacket();
			return;
#endif
	}

	RecvErrorPacket(header);
}

void CPythonNetworkStream::SetHandShakePhase()
{
	SetHandShakePhaseNoPython();

	if (!__DirectEnterMode_IsSet())
	{
		PyCallClassMemberFunc(m_apoPhaseWnd[PHASE_WINDOW_LOGIN], "OnHandShake", Py_BuildValue("()"));
	}
}

// P0 2026-08-14 (handshake silencioso del cliente real, KEEP_ACCOUNT_CONNETION_ENABLE=1):
// el canal se conecta con Connect() crudo desde CAccountConnector (AccountConnector.cpp
// __AuthState_RecvAuthSuccess) tras el GC_AUTH_SUCCESS. Sin fase preparada, el
// RecvPhasePacket -> SetHandShakePhase() llamaba OnHandShake contra la ventana de login
// YA CERRADA post-auth -> excepcion Python silenciosa -> ni eco ni LOGIN3 (canal mudo).
// Esta variante setea la fase HandShake sin el callback Python (equivalente al modo
// DirectEnter: PythonNetworkStreamPhaseHandShake.cpp:95-102).
void CPythonNetworkStream::SetHandShakePhaseNoPython()
{
	if ("HandShake"!=m_strPhase)
		m_phaseLeaveFunc.Run();

	Tracen("");
	Tracen("## Network - Hand Shake Phase ##");
	Tracen("");

	m_strPhase = "HandShake";

	m_dwChangingPhaseTime = ELTimer_GetMSec();
	m_phaseProcessFunc.Set(this, &CPythonNetworkStream::HandShakePhase);
	m_phaseLeaveFunc.Set(this, &CPythonNetworkStream::__LeaveHandshakePhase);

	SetGameOnline();
}

bool CPythonNetworkStream::RecvHandshakePacket()
{
	TPacketGCHandshake kHandshakeData;
	if (!Recv(sizeof(TPacketGCHandshake), &kHandshakeData))
		return false;

	Tracenf("HANDSHAKE RECV %u %d", kHandshakeData.dwTime, kHandshakeData.lDelta);

	m_kServerTimeSync.m_dwChangeServerTime = kHandshakeData.dwTime + kHandshakeData.lDelta;
	m_kServerTimeSync.m_dwChangeClientTime = ELTimer_GetMSec();

	kHandshakeData.dwTime = kHandshakeData.dwTime + kHandshakeData.lDelta + kHandshakeData.lDelta;
	kHandshakeData.lDelta = 0;

	Tracenf("HANDSHAKE SEND %u", kHandshakeData.dwTime);

	kHandshakeData.header = HEADER_CG_TIME_SYNC;
	if (!Send(sizeof(TPacketGCHandshake), &kHandshakeData))
	{
		assert(!"Failed Sending Handshake");
		return false;
	}

	SendSequence();

	return true;
}

bool CPythonNetworkStream::RecvHandshakeOKPacket()
{
	TPacketGCBlank kBlankPacket;
	if (!Recv(sizeof(TPacketGCBlank), &kBlankPacket))
		return false;

	const DWORD dwDelta=ELTimer_GetMSec()-m_kServerTimeSync.m_dwChangeClientTime;
	ELTimer_SetServerMSec(m_kServerTimeSync.m_dwChangeServerTime+dwDelta);

	Tracenf("HANDSHAKE OK RECV %u %u", m_kServerTimeSync.m_dwChangeServerTime, dwDelta);

	return true;
}

bool CPythonNetworkStream::RecvHybridCryptKeyPacket()
{
	const int iFixedHeaderSize = TPacketGCHybridCryptKeys::GetFixedHeaderSize();

	TDynamicSizePacketHeader header;
	if( !Peek( sizeof(header), &header) )
		return false;

	TPacketGCHybridCryptKeys kPacket(header.size-iFixedHeaderSize);

	if (!Recv(iFixedHeaderSize, &kPacket))
		return false;

	if (!Recv(kPacket.iKeyStreamLen, kPacket.m_pStream))
		return false;

	CEterPackManager::Instance().RetrieveHybridCryptPackKeys( kPacket.m_pStream );
	return true;
}

bool CPythonNetworkStream::RecvHybridCryptSDBPacket()
{
	const int iFixedHeaderSize = TPacketGCHybridSDB::GetFixedHeaderSize();

	TDynamicSizePacketHeader header;
	if( !Peek( sizeof(header), &header) )
		return false;

	TPacketGCHybridSDB kPacket(header.size-iFixedHeaderSize);

	if (!Recv(iFixedHeaderSize, &kPacket))
		return false;

	if (!Recv(kPacket.iSDBStreamLen, kPacket.m_pStream))
		return false;

	CEterPackManager::Instance().RetrieveHybridCryptPackSDB( kPacket.m_pStream );
	return true;
}

#ifdef _IMPROVED_PACKET_ENCRYPTION_
bool CPythonNetworkStream::RecvKeyAgreementPacket()
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
			assert(!"Failed Sending KeyAgreement");
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

bool CPythonNetworkStream::RecvKeyAgreementCompletedPacket()
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

