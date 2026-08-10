#ifndef __CLIENT_H__
#define __CLIENT_H__

#include "../../common/stl.h"
#include "desc.h"

class CLIENT_DESC : public DESC
{
	public:
		CLIENT_DESC();
		virtual ~CLIENT_DESC();

		virtual BYTE	GetType() { return DESC_TYPE_CONNECTOR; }
		virtual void	Destroy();
		virtual void	SetPhase(int phase);

		bool 		Connect(int iPhaseWhenSucceed = 0);
		void		Setup(LPFDWATCH _fdw, const char * _host, WORD _port);

		void		SetRetryWhenClosed(bool);

		void		DBPacketHeader(BYTE bHeader, DWORD dwHandle, DWORD dwSize);
		void		DBPacket(BYTE bHeader, DWORD dwHandle, const void * c_pvData, DWORD dwSize);
		void		Packet(const void * c_pvData, int iSize);
		bool		IsRetryWhenClosed() const;

		void		Update(DWORD t);
		void		UpdateChannelStatus(DWORD t, bool fForce);

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void DBPacket(BYTE bHeader, DWORD dwHandle, const T& c_pvData) {
			DBPacket(bHeader, dwHandle, &c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void DBPacket(BYTE bHeader, DWORD dwHandle, const C& v) {
			DBPacket(bHeader, dwHandle, v.data(), v.size() * sizeof(typename C::value_type));
		}

		template<typename T, std::enable_if_t<utils::IsRawV<T>>* = nullptr>
		void Packet(const T& c_pvData) {
			Packet(&c_pvData, sizeof(T));
		}
		template<typename C, std::enable_if_t<utils::IsContiguousV<C>>* = nullptr>
		void Packet(const C& v) {
			Packet(v.data(), v.size() * sizeof(typename C::value_type));
		}

		// Non-destructive close for reuse
		void Reset();

	private:
		void InitializeBuffers();

	protected:
		int			m_iPhaseWhenSucceed;
		bool		m_bRetryWhenClosed;
		time_t		m_LastTryToConnectTime;
		time_t		m_tLastChannelStatusUpdateTime;

		CInputDB 	m_inputDB;
		CInputP2P 	m_inputP2P;
};

extern LPCLIENT_DESC db_clientdesc;
extern LPCLIENT_DESC g_pkAuthMasterDesc;

#endif

