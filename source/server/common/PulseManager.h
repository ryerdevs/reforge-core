// martysama0134's PulseManager class for quick anti-flood code implementation
#ifndef __COMMON__PULSEMANAGER_H__
#define __COMMON__PULSEMANAGER_H__
#pragma once

#include <cstdint>
#include <ctime>
#include <unordered_map>
#include <chrono>

enum class ePulse {
	GuildComment,
	CommandRequest,
	ItemDrop,
	BoxOpening,
	BuySell,
	SafeboxMove,
	ChannelStatus,
	RideMount,
	SharedRequest,
};

// #define __PULSEMANAGER__SECOND_SUPPORT__
#define __PULSEMANAGER__CLOCK_SUPPORT__
// #define __PULSEMANAGER__M2_SUPPORT__

#ifdef __PULSEMANAGER__M2_SUPPORT__
#include "../libthecore/include/stdafx.h"
#endif

#define PULSEMANAGER_CLOCK_TO_SEC(diff) (std::chrono::duration_cast<std::chrono::milliseconds>(diff).count()/1000.0)
#define PULSEMANAGER_CLOCK_TO_SEC2(key1, key2) (PULSEMANAGER_CLOCK_TO_SEC(PulseManager::Instance().DiffClock(key1, key2)))

class PulseManager
{
public:
	using SubKeyT = ePulse;
	using MainKeyT = uint32_t;

#ifdef __PULSEMANAGER__M2_SUPPORT__
	using PulseT = int;
	using PulseMapT = std::unordered_map<SubKeyT, PulseT>;
	using MainPulseMapT = std::unordered_map<MainKeyT, PulseMapT>;
	PulseT passesPerSec{ 25 };
	MainPulseMapT pulseMap;
#endif

#ifdef __PULSEMANAGER__SECOND_SUPPORT__
	using SecondT = time_t;//uint32_t;
	using SecondMapT = std::unordered_map<SubKeyT, SecondT>;
	using MainSecondMapT = std::unordered_map<MainKeyT, SecondMapT>;
	MainSecondMapT secondMap;
#endif

#ifdef __PULSEMANAGER__CLOCK_SUPPORT__
	using TypeClock = std::chrono::high_resolution_clock;
	using ClockT = TypeClock::time_point;
	using DurationT = TypeClock::duration;
	using CountT = int64_t;
	using PairValueT = std::pair<DurationT, CountT>;
	using ClockMapT = std::unordered_map<SubKeyT, PairValueT>;
	using MainClockMapT = std::unordered_map<MainKeyT, ClockMapT>;
	MainClockMapT clockMap;
#endif

	static PulseManager& Instance() {
		thread_local PulseManager _this;
		return _this;
	}

#ifdef __PULSEMANAGER__SECOND_SUPPORT__
	inline static SecondT GetTime() {
		return time(0); //get_global_time();//get_dword_time();
	}

	/* SECOND BLOCK */
	SecondT GetSecond(MainKeyT key1, SubKeyT key2) {
		auto it1 = secondMap.find(key1);
		if (it1 == secondMap.end())
			return 0;
		auto it2 = it1->second.find(key2);
		if (it2 == it1->second.end())
			return 0;

		return it2->second;
	}

	void SetSecond(MainKeyT key1, SubKeyT key2, SecondT value, bool appendCurrent = true) {
		if (appendCurrent)
			value += GetTime();
		secondMap[key1][key2] = value;
	}

	bool CheckSecond(MainKeyT key1, SubKeyT key2, SecondT nextLapse) {
		if (GetSecond(key1, key2) > GetTime())
			return false;
		SetSecond(key1, key2, nextLapse, true);
		return true;
	}
#endif

#ifdef __PULSEMANAGER__CLOCK_SUPPORT__
	inline static DurationT GetChrono() {
		return std::chrono::high_resolution_clock::now().time_since_epoch();
	}

	/* CLOCK BLOCK */
	PairValueT GetPair(MainKeyT key1, SubKeyT key2) const {
		const auto it1 = clockMap.find(key1);
		if (it1 == clockMap.end())
			return {TypeClock::duration::zero(), 0};

		const auto it2 = it1->second.find(key2);
		if (it2 == it1->second.end())
			return {TypeClock::duration::zero(), 0};

		return it2->second;
	}

	DurationT GetClock(MainKeyT key1, SubKeyT key2) const {
		return GetPair(key1, key2).first;
	}

	CountT GetCount(MainKeyT key1, SubKeyT key2) const {
		return GetPair(key1, key2).second;
	}

	void SetPair(MainKeyT key1, SubKeyT key2, PairValueT pair, bool appendCurrent = true) {
		if (appendCurrent)
			pair.first += GetChrono();
		clockMap[key1][key2] = pair;
	}

	void SetClock(MainKeyT key1, SubKeyT key2, DurationT value, bool appendCurrent = true) {
		if (appendCurrent)
			value += GetChrono();
		clockMap[key1][key2].first = value;
	}

	void SetCount(MainKeyT key1, SubKeyT key2, CountT count, bool decreaseValue = true) {
		if (decreaseValue)
			clockMap[key1][key2].second -= count;
		else
			clockMap[key1][key2].second = count;
	}

	bool CheckClock(MainKeyT key1, SubKeyT key2) {
		if (GetClock(key1, key2) > GetChrono())
			return false;
		return true;
	}

	bool IncreaseClock(MainKeyT key1, SubKeyT key2, DurationT nextLapse) {
		if (!CheckClock(key1, key2))
			return false;
		SetClock(key1, key2, nextLapse, true);
		return true;
	}

	bool CheckCount(MainKeyT key1, SubKeyT key2) {
		//std::cout << "CHECK COUNT " << GetCount(key1, key2) << std::endl;
		SetCount(key1, key2, 1, true);
		if (GetCount(key1, key2) <= 0)
			return false;
		return true;
	}

	bool IncreaseCount(MainKeyT key1, SubKeyT key2, DurationT nextLapse, CountT maxCount) {
		if (!CheckClock(key1, key2)) { // if clock fails, check if max count is reached
			if (!CheckCount(key1, key2))
				return false;
			return true;
		}
		SetPair(key1, key2, { nextLapse, maxCount }, true);
		return true;
	}

	DurationT DiffClock(MainKeyT key1, SubKeyT key2) {
		return GetClock(key1, key2) - GetChrono();
	}

	void ClearClock(MainKeyT key1) {
		auto it1 = clockMap.find(key1);
		if (it1 == clockMap.end())
			return;
		clockMap.erase(it1);
	}

	void ClearClock(MainKeyT key1, SubKeyT key2) {
		auto it1 = clockMap.find(key1);
		if (it1 == clockMap.end())
			return;
		auto it2 = it1->second.find(key2);
		if (it2 == it1->second.end())
			return;
		it1->second.erase(it2);
	}
#endif

#ifdef __PULSEMANAGER__M2_SUPPORT__
	void SetPassesPerSec(PulseT v) {
		passesPerSec = v;
	}

	PulseT Sec2Pulse(SecondT v) {
		return v * passesPerSec;
	}

	SecondT Pulse2Sec(PulseT v) {
		return v / passesPerSec;
	}

	inline static PulseT GetCorePulse() {
		return thecore_pulse();
	}

	/* CLOCK BLOCK */
	bool HasPulse(MainKeyT key1, SubKeyT key2) {
		auto it1 = pulseMap.find(key1);
		if (it1 == pulseMap.end())
			return false;
		auto it2 = it1->second.find(key2);
		if (it2 == it1->second.end())
			return false;
		return true;
	}

	PulseT GetPulse(MainKeyT key1, SubKeyT key2) {
		auto it1 = pulseMap.find(key1);
		if (it1 == pulseMap.end())
			return 0;
		auto it2 = it1->second.find(key2);
		if (it2 == it1->second.end())
			return 0;
		return it2->second;
	}

	void SetPulse(MainKeyT key1, SubKeyT key2, PulseT value, bool appendCurrent = true) {
		if (appendCurrent)
			value += GetCorePulse();
		pulseMap[key1][key2] = value;
	}

	bool CheckPulse(MainKeyT key1, SubKeyT key2, PulseT nextLapse) {
		if (GetPulse(key1, key2) > GetCorePulse())
			return false;
		SetPulse(key1, key2, nextLapse, true);
		return true;
	}

	SecondT GetPSecond(MainKeyT key1, SubKeyT key2) {
		auto v = GetPulse(key1, key2);
		return Pulse2Sec(v);
	}

	void SetPSecond(MainKeyT key1, SubKeyT key2, SecondT value, bool appendCurrent = true) {
		auto pulses = Sec2Pulse(value);
		// if (appendCurrent)
		// 	pulses += GetTime();
		SetPulse(key1, key2, pulses, true);
	}
#endif

};
#endif // __COMMON__PULSEMANAGER_H__

