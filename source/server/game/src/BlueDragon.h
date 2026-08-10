#ifndef BLUE_DRAGON_H
#define BLUE_DRAGON_H
#pragma once

#define ENABLE_BLUEDRAGON_RENEWAL

namespace BlueDragon {
	constexpr int BossVnum = 2493;
	constexpr int StoneCount = 4;
}

extern int BlueDragon_StateBattle (LPCHARACTER);
extern time_t UseBlueDragonSkill (LPCHARACTER, unsigned int);
extern int BlueDragon_Damage (LPCHARACTER me, LPCHARACTER attacker, int dam);

#endif // BLUE_DRAGON_H

