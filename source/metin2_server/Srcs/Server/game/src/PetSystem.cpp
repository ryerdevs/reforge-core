#include "stdafx.h"
#include "config.h"
#include "utils.h"
#include "vector.h"
#include "char.h"
#include "sectree_manager.h"
#include "char_manager.h"
#include "mob_manager.h"
#include "PetSystem.h"
#include "../../common/VnumHelper.h"
#include "packet.h"
#include "item_manager.h"
#include "item.h"

EVENTINFO(petsystem_event_info)
{
	CPetSystem* pPetSystem;
};

EVENTFUNC(petsystem_update_event)
{
	const auto info = dynamic_cast<petsystem_event_info*>( event->info );
	if ( info == nullptr)
	{
		sys_err( "check_speedhack_event> <Factor> Null pointer" );
		return 0;
	}

	CPetSystem*	pPetSystem = info->pPetSystem;

	if (nullptr == pPetSystem)
		return 0;

	pPetSystem->Update(0);

	return PASSES_PER_SEC(1) / 4;
}

///////////////////////////////////////////////////////////////////////////////////////
//  CPetActor
///////////////////////////////////////////////////////////////////////////////////////

CPetActor::CPetActor(LPCHARACTER owner, DWORD vnum, DWORD options)
{
	m_dwVnum = vnum;
	m_dwVID = 0;
	m_dwOptions = options;
	m_dwLastActionTime = 0;

	m_pkChar = nullptr;
	m_pkOwner = owner;

	m_originalMoveSpeed = 0;

	m_dwSummonItemVID = 0;
	m_dwSummonItemVnum = 0;
}

CPetActor::~CPetActor()
{
	this->Unsummon(
		#ifdef USE_ACTIVE_PET_SEAL_EFFECT
		false
		#endif
	);

	m_pkOwner = nullptr;
}

void CPetActor::SetName(const char* name)
{
	std::string petName = m_pkOwner->GetName();
	petName += "'s ";
	petName += (name) ? name : "Pet";

	if (IsSummoned())
		m_pkChar->SetName(petName);

	m_name = petName;
}

bool CPetActor::Mount() const
{
	if (nullptr == m_pkOwner)
		return false;

	if (true == HasOption(EPetOption_Mountable))
		m_pkOwner->MountVnum(m_dwVnum);

	return m_pkOwner->GetMountVnum() == m_dwVnum;
}

void CPetActor::Unmount() const
{
	if (nullptr == m_pkOwner)
		return;

	if (m_pkOwner->IsHorseRiding())
		m_pkOwner->StopRiding();
}

void CPetActor::Unsummon(
	#ifdef USE_ACTIVE_PET_SEAL_EFFECT
	bool updateSocket
	#endif
)
{
	if (true == this->IsSummoned())
	{
		this->ClearBuff();
		this->SetSummonItem(nullptr
		                    #ifdef USE_ACTIVE_PET_SEAL_EFFECT
		                    , updateSocket
		                    #endif
		);

		if (nullptr != m_pkOwner)
			m_pkOwner->ComputePoints();

		if (nullptr != m_pkChar)
			M2_DESTROY_CHARACTER(m_pkChar);

		m_pkChar = nullptr;
		m_dwVID = 0;
	}
}

DWORD CPetActor::Summon(const char* petName, LPITEM pSummonItem, bool bSpawnFar)
{
	long x = m_pkOwner->GetX();
	long y = m_pkOwner->GetY();
	const long z = m_pkOwner->GetZ();

	if (true == bSpawnFar)
	{
		x += (number(0, 1) * 2 - 1) * number(2000, 2500);
		y += (number(0, 1) * 2 - 1) * number(2000, 2500);
	}
	else
	{
		x += number(-100, 100);
		y += number(-100, 100);
	}

	if (nullptr != m_pkChar)
	{
		m_pkChar->Show (m_pkOwner->GetMapIndex(), x, y);
		m_dwVID = m_pkChar->GetVID();

		return m_dwVID;
	}

	m_pkChar = CHARACTER_MANAGER::instance().SpawnMob(
				m_dwVnum,
				m_pkOwner->GetMapIndex(),
				x, y, z,
				false, (int)(m_pkOwner->GetRotation()+180), false);

	if (nullptr == m_pkChar)
	{
		sys_err("[CPetSystem::Summon] Failed to summon the pet. (vnum: %d)", m_dwVnum);
		return 0;
	}

	#ifdef __PET_SYSTEM__
	m_pkChar->SetPet();
	#endif

	m_pkChar->SetEmpire(m_pkOwner->GetEmpire());

	m_dwVID = m_pkChar->GetVID();

	this->SetName(petName);

	this->SetSummonItem(pSummonItem);
	m_pkOwner->ComputePoints();
	m_pkChar->Show(m_pkOwner->GetMapIndex(), x, y, z);

	return m_dwVID;
}

bool CPetActor::_UpdatAloneActionAI(float fMinDist, float fMaxDist)
{
	const float fDist = number(fMinDist, fMaxDist);
	const float r = (float)number (0, 359);
	const float dest_x = GetOwner()->GetX() + fDist * cos(r);
	const float dest_y = GetOwner()->GetY() + fDist * sin(r);

	m_pkChar->SetNowWalking(true);

	if (!m_pkChar->IsStateMove() && m_pkChar->Goto(dest_x, dest_y))
		m_pkChar->SendMovePacket(FUNC_WAIT, 0, 0, 0, 0);

	m_dwLastActionTime = get_dword_time();

	return true;
}

bool CPetActor::_UpdateFollowAI()
{
	if (nullptr == m_pkChar->m_pkMobData)
	{
		//sys_err("[CPetActor::_UpdateFollowAI] m_pkChar->m_pkMobData is Null");
		return false;
	}

	if (0 == m_originalMoveSpeed)
	{
		const CMob* mobData = CMobManager::Instance().Get(m_dwVnum);

		if (nullptr != mobData)
			m_originalMoveSpeed = mobData->m_table.sMovingSpeed;
	}
	const float	START_FOLLOW_DISTANCE = 300.0f;
	const float	START_RUN_DISTANCE = 900.0f;

	const float	RESPAWN_DISTANCE = 4500.f;
	const int		APPROACH = 200;

	bool bRun = false;

	const DWORD currentTime = get_dword_time();

	const long ownerX = m_pkOwner->GetX();
	const long ownerY = m_pkOwner->GetY();
	const long charX = m_pkChar->GetX();
	const long charY = m_pkChar->GetY();

	const float fDist = DISTANCE_APPROX(charX - ownerX, charY - ownerY);

	if (fDist >= RESPAWN_DISTANCE)
	{
		const float fOwnerRot = m_pkOwner->GetRotation() * 3.141592f / 180.f;
		const float fx = -APPROACH * cos(fOwnerRot);
		const float fy = -APPROACH * sin(fOwnerRot);
		if (m_pkChar->Show(m_pkOwner->GetMapIndex(), ownerX + fx, ownerY + fy))
		{
			return true;
		}
	}

	if (fDist >= START_FOLLOW_DISTANCE)
	{
		if( fDist >= START_RUN_DISTANCE)
		{
			bRun = true;
		}

		m_pkChar->SetNowWalking(!bRun);

		Follow(APPROACH);

		m_pkChar->SetLastAttacked(currentTime);
		m_dwLastActionTime = currentTime;
	}

	else
		m_pkChar->SendMovePacket(FUNC_WAIT, 0, 0, 0, 0);

	return true;
}

bool CPetActor::Update(DWORD deltaTime)
{
	bool bResult = true;

	if (m_pkOwner->IsDead() || (IsSummoned() && m_pkChar->IsDead())
		|| nullptr == ITEM_MANAGER::instance().FindByVID(this->GetSummonItemVID())
		|| ITEM_MANAGER::instance().FindByVID(this->GetSummonItemVID())->GetOwner() != this->GetOwner()
		)
	{
		this->Unsummon();
		return true;
	}

	if (this->IsSummoned() && HasOption(EPetOption_Followable))
		bResult = bResult && this->_UpdateFollowAI();

	return bResult;
}

bool CPetActor::Follow(float fMinDistance) const
{
	if( !m_pkOwner || !m_pkChar)
		return false;

	const float fOwnerX = m_pkOwner->GetX();
	const float fOwnerY = m_pkOwner->GetY();

	const float fPetX = m_pkChar->GetX();
	const float fPetY = m_pkChar->GetY();

	const float fDist = DISTANCE_SQRT(fOwnerX - fPetX, fOwnerY - fPetY);
	if (fDist <= fMinDistance)
		return false;

	m_pkChar->SetRotationToXY(fOwnerX, fOwnerY);

	float fx, fy;

	const float fDistToGo = fDist - fMinDistance;
	GetDeltaByDegree(m_pkChar->GetRotation(), fDistToGo, &fx, &fy);

	if (!m_pkChar->Goto((int)(fPetX+fx+0.5f), (int)(fPetY+fy+0.5f)) )
		return false;

	m_pkChar->SendMovePacket(FUNC_WAIT, 0, 0, 0, 0, 0);

	return true;
}

bool CPetActor::SetSummonItem(LPITEM pItem
	#ifdef USE_ACTIVE_PET_SEAL_EFFECT
	, bool updateSocket
	#endif
)
{
	if (!pItem)
	{
		#ifdef USE_ACTIVE_PET_SEAL_EFFECT
		const LPITEM pSummonItem = ITEM_MANAGER::instance().FindByVID(m_dwSummonItemVID);
		if (pSummonItem && updateSocket)
			pSummonItem->SetSocket(PET_SEAL_ACTIVE_SOCKET_IDX, false);
		#endif
		m_dwSummonItemVID = 0;
		m_dwSummonItemVnum = 0;
		return false;
	}
	#ifdef USE_ACTIVE_PET_SEAL_EFFECT
	pItem->SetSocket(PET_SEAL_ACTIVE_SOCKET_IDX, true);
	#endif
	m_dwSummonItemVID = pItem->GetVID();
	m_dwSummonItemVnum = pItem->GetVnum();
	return true;
}

bool __PetCheckBuff(const CPetActor* pPetActor)
{
	bool bMustHaveBuff = true;
	switch (pPetActor->GetVnum())
	{
		case 34004:
		case 34009:
			if (!pPetActor->GetOwner()->GetDungeon())
				bMustHaveBuff = false;
		default:
			break;
	}
	return bMustHaveBuff;
}

void CPetActor::GiveBuff() const
{
	if (!__PetCheckBuff(this))
		return;
	const LPITEM item = ITEM_MANAGER::instance().FindByVID(m_dwSummonItemVID);
	if (item)
		item->ModifyPoints(true);
	return ;
}

void CPetActor::ClearBuff() const
{
	if (nullptr == m_pkOwner)
		return ;
	const TItemTable* item_proto = ITEM_MANAGER::instance().GetTable(m_dwSummonItemVnum);
	if (nullptr == item_proto)
		return;
	if (!__PetCheckBuff(this)) // @fixme129
		return;
	for (int i = 0; i < ITEM_APPLY_MAX_NUM; i++)
	{
		if (item_proto->aApplies[i].bType == APPLY_NONE)
			continue;
		m_pkOwner->ApplyPoint(item_proto->aApplies[i].bType, -item_proto->aApplies[i].lValue);
	}

	return ;
}

///////////////////////////////////////////////////////////////////////////////////////
//  CPetSystem
///////////////////////////////////////////////////////////////////////////////////////

CPetSystem::CPetSystem(LPCHARACTER owner)
{
	// assert(0 != owner && "[CPetSystem::CPetSystem] Invalid owner");

	m_pkOwner = owner;
	m_dwUpdatePeriod = 400;

	m_dwLastUpdateTime = 0;
}

CPetSystem::~CPetSystem()
{
	Destroy();
}

void CPetSystem::Destroy()
{
	for (auto iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		const CPetActor* petActor = iter->second;

		if (nullptr != petActor)
		{
			delete petActor;
		}
	}
	event_cancel(&m_pkPetSystemUpdateEvent);
	m_petActorMap.clear();
}

bool CPetSystem::Update(DWORD deltaTime)
{
	bool bResult = true;

	const DWORD currentTime = get_dword_time();

	if (m_dwUpdatePeriod > currentTime - m_dwLastUpdateTime)
		return true;

	std::vector <CPetActor*> v_garbageActor;

	for (auto iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		CPetActor* petActor = iter->second;

		if (nullptr != petActor && petActor->IsSummoned())
		{
			const LPCHARACTER pPet = petActor->GetCharacter();

			if (nullptr == CHARACTER_MANAGER::instance().Find(pPet->GetVID()))
			{
				v_garbageActor.emplace_back(petActor);
			}
			else
			{
				bResult = bResult && petActor->Update(deltaTime);
			}
		}
	}
	for (auto it = v_garbageActor.begin(); it != v_garbageActor.end(); it++)
		DeletePet(*it);

	m_dwLastUpdateTime = currentTime;

	return bResult;
}

void CPetSystem::DeletePet(DWORD mobVnum)
{
	const auto iter = m_petActorMap.find(mobVnum);

	if (m_petActorMap.end() == iter)
	{
		sys_err("[CPetSystem::DeletePet] Can't find pet on my list (VNUM: %d)", mobVnum);
		return;
	}

	const CPetActor* petActor = iter->second;

	if (nullptr == petActor)
		sys_err("[CPetSystem::DeletePet] Null Pointer (petActor)");
	else
		delete petActor;

	m_petActorMap.erase(iter);
}

void CPetSystem::DeletePet(CPetActor* petActor)
{
	for (auto iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		if (iter->second == petActor)
		{
			delete petActor;
			m_petActorMap.erase(iter);

			return;
		}
	}

	sys_err("[CPetSystem::DeletePet] Can't find petActor(0x%x) on my list(size: %d) ", petActor, m_petActorMap.size());
}

void CPetSystem::Unsummon(DWORD vnum, bool bDeleteFromList)
{
	CPetActor* actor = this->GetByVnum(vnum);

	if (nullptr == actor)
	{
		sys_err("[CPetSystem::GetByVnum(%d)] Null Pointer (petActor)", vnum);
		return;
	}
	actor->Unsummon();

	if (true == bDeleteFromList)
		this->DeletePet(actor);

	bool bActive = false;
	for (auto it = m_petActorMap.begin(); it != m_petActorMap.end(); it++)
	{
		bActive |= it->second->IsSummoned();
	}
	if (false == bActive)
	{
		event_cancel(&m_pkPetSystemUpdateEvent);
		m_pkPetSystemUpdateEvent = nullptr;
	}
}

void CPetSystem::UnsummonAll()
{
	for (const auto & iter : m_petActorMap)
	{
		auto * actor = iter.second;
		if (actor)
			actor->Unsummon();
	}

	bool bActive = false;
	for (const auto & it : m_petActorMap)
		bActive |= it.second->IsSummoned();
	if (!bActive)
	{
		event_cancel(&m_pkPetSystemUpdateEvent);
		m_pkPetSystemUpdateEvent = nullptr;
	}
}

CPetActor* CPetSystem::Summon(DWORD mobVnum, LPITEM pSummonItem, const char* petName, bool bSpawnFar, DWORD options)
{
	CPetActor* petActor = this->GetByVnum(mobVnum);

	if (nullptr == petActor)
	{
		petActor = M2_NEW CPetActor(m_pkOwner, mobVnum, options);
		m_petActorMap.emplace(mobVnum, petActor);
	}

#ifdef ENABLE_NEWSTUFF
	const DWORD petVID = petActor->Summon(petName, pSummonItem, bSpawnFar);
	if (!petVID)
		sys_err("[CPetSystem::Summon(%d)] Null Pointer (petVID)", pSummonItem);
#endif

	if (nullptr == m_pkPetSystemUpdateEvent)
	{
		petsystem_event_info* info = AllocEventInfo<petsystem_event_info>();

		info->pPetSystem = this;

		m_pkPetSystemUpdateEvent = event_create(petsystem_update_event, info, PASSES_PER_SEC(1) / 4);
	}

	return petActor;
}

CPetActor* CPetSystem::GetByVID(DWORD vid) const
{
	CPetActor* petActor = nullptr;

	bool bFound = false;

	for (auto iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		petActor = iter->second;

		if (nullptr == petActor)
		{
			sys_err("[CPetSystem::GetByVID(%d)] Null Pointer (petActor)", vid);
			continue;
		}

		bFound = petActor->GetVID() == vid;

		if (true == bFound)
			break;
	}

	return bFound ? petActor : nullptr;
}

CPetActor* CPetSystem::GetByVnum(DWORD vnum) const
{
	CPetActor* petActor = nullptr;

	const auto iter = m_petActorMap.find(vnum);

	if (m_petActorMap.end() != iter)
		petActor = iter->second;

	return petActor;
}

size_t CPetSystem::CountSummoned() const
{
	size_t count = 0;

	for (auto iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		const CPetActor* petActor = iter->second;

		if (nullptr != petActor)
		{
			if (petActor->IsSummoned())
				++count;
		}
	}

	return count;
}

void CPetSystem::RefreshBuff()
{
	for (TPetActorMap::const_iterator iter = m_petActorMap.begin(); iter != m_petActorMap.end(); ++iter)
	{
		const CPetActor* petActor = iter->second;
		if (petActor && petActor->IsSummoned())
			petActor->GiveBuff();
	}
}

