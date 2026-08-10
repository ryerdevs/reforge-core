#include "stdafx.h"
#include "config.h"
#include "map_location.h"
#include "sectree_manager.h"

CMapLocation g_mapLocations;

#ifdef ENABLE_MOVE_CHANNEL
bool CMapLocation::Get(long x, long y, long &lIndex, long &lAddr, WORD &wPort, BYTE channel)
{
	if (channel == 0)
		channel = g_bChannel;
	lIndex = SECTREE_MANAGER::instance().GetMapIndex(x, y);
	return Get(lIndex, lAddr, wPort, channel) || Get(lIndex, lAddr, wPort, (channel==99) ? 1 : 99);
}

bool CMapLocation::Get(long iIndex, long &lAddr, WORD &wPort, BYTE channel)
{
	// sys_log(0, "CMapLocation::Get - OK MapIndex[%d] channel %d", iIndex, channel);
	if (iIndex == 0) {
		// sys_log(0, "CMapLocation::Get - Error MapIndex[%d] channel %d", iIndex, channel);
		return false;
	}

	const auto key = std::make_pair(channel, iIndex);
	const auto it = m_map_address.find(key);
	if (m_map_address.end() == it) {
		// sys_log(0, "CMapLocation::Get - Error MapIndex[%d]", iIndex);
		// for (auto & i : m_map_address)
		// 	sys_log(0, "Map(%d:%d): Server(%s:%d)", i.first.first, i.first.second, inet_ntoa((in_addr)i.second.addr), i.second.port);
		return false;
	}

	lAddr = it->second.addr;
	wPort = it->second.port;
	return true;
}

void CMapLocation::Insert(long lIndex, const char *c_pszHost, WORD wPort, BYTE channel)
{
	TLocation loc{};
	loc.addr = inet_addr(c_pszHost);
	loc.port = wPort;

	auto key = std::make_pair(channel, lIndex);
	m_map_address.emplace(key, loc);
	sys_log(0, "MapLocation::Insert: %d %d %s %d", channel, lIndex, c_pszHost, wPort);
}

#else
bool CMapLocation::Get(long x, long y, long &lIndex, long &lAddr, WORD &wPort)
{
	lIndex = SECTREE_MANAGER::instance().GetMapIndex(x, y);
	return Get(lIndex, lAddr, wPort);
}

bool CMapLocation::Get(long iIndex, long &lAddr, WORD &wPort)
{
	if (iIndex == 0) {
		sys_log(0, "CMapLocation::Get - Error MapIndex[%d]", iIndex);
		return false;
	}

	auto it = m_map_address.find(iIndex);
	if (m_map_address.end() == it) {
		sys_log(0, "CMapLocation::Get - Error MapIndex[%d]", iIndex);
		for (auto & i : m_map_address)
			sys_log(0, "Map(%d): Server(%x:%d)", i.first, i.second.addr, i.second.port);
		return false;
	}

	lAddr = it->second.addr;
	wPort = it->second.port;
	return true;
}

void CMapLocation::Insert(long lIndex, const char *c_pszHost, WORD wPort)
{
	TLocation loc{};
	loc.addr = inet_addr(c_pszHost);
	loc.port = wPort;

	m_map_address.emplace(lIndex, loc);
	sys_log(0, "MapLocation::Insert : %d %s %d", lIndex, c_pszHost, wPort);
}
#endif

