#include "../../common/stl.h"
#include "../../common/CommonDefines.h"

class CMapLocation : public singleton<CMapLocation>
{
#ifdef ENABLE_MOVE_CHANNEL
	public:
		using PairLocation = std::pair<BYTE, WORD>;
		typedef struct SLocation
		{
			int		addr{};
			WORD	port{};
			BYTE	channel{};
		} TLocation;

		bool    Get(long x, long y, long &lMapIndex, long &lAddr, WORD &wPort, BYTE channel = 0);
		bool	Get(long iIndex, long &lAddr, WORD &wPort, BYTE channel);
		void    Insert(long lIndex, const char *c_pszHost, WORD wPort, BYTE channel);

	protected:
		std::map<PairLocation, TLocation> m_map_address;
#else
	public:
		typedef struct SLocation
		{
			long        addr;
			WORD        port;
		} TLocation;

		bool    Get(long x, long y, long &lMapIndex, long &lAddr, WORD &wPort);
		bool	Get(long iIndex, long &lAddr, WORD &wPort);
		void    Insert(long lIndex, const char *c_pszHost, WORD wPort);

	protected:
		std::map<long, TLocation> m_map_address;
#endif

};

