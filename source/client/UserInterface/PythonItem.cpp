#include "stdafx.h"
#include "../eterlib/GrpMath.h"
#include "../gamelib/ItemManager.h"
#include "../EffectLib/EffectManager.h"
#include "PythonBackground.h"

#include "pythonitem.h"
#include "PythonTextTail.h"

#include "PythonSkill.h"
#include <fmt/fmt.h>

constexpr float c_fDropStartHeight = 100.0f;
constexpr float c_fDropTime = 0.5f;

std::string CPythonItem::TGroundItemInstance::ms_astDropSoundFileName[DROPSOUND_NUM];

template <typename T>
int CountDigit(T n)
{
	if (n == 0) return 1;
	int count = 0;
	while (n != 0) {
		n = n / 10;
		++count;
	}
	return count;
}

std::string NumberToK(int64_t n)
{
	std::string result;
	bool isNegative = n < 0; // keep negative information
	n = std::abs(n); // reset sign

	int64_t tempNumber = 0;
	int64_t countDigits = 0;
	do {
		tempNumber = n % 1000;
		//std::cout << ((isNegative && result.empty()) ? "-" : "") << n << '\n';

		// skip zero and check if kappable
		if (n && (tempNumber) == 0) {
			result = result + "k";
			n /= 1000;
		}
		else {
			countDigits = CountDigit(tempNumber);
			n /= 1000;
			// pad with zeroes
			if (n != 0) {
				// don't append dot if empty or previously kapped
				if (result.empty() || result[0] == 'k')
					result = fmt::format("{:03d}{}", tempNumber, result);
				else
					result = fmt::format("{:03d}.{}", tempNumber, result);
			}
			else {
				// don't append dot if empty or previously kapped
				if (result.empty() || result[0] == 'k')
					result = fmt::format("{}{}", tempNumber, result);
				else
					result = fmt::format("{}.{}", tempNumber, result);
			}
		}
	} while (n != 0);

	// apply sign
	if (isNegative)
		result = std::string("-") + result;
	return result;
}

void CPythonItem::GetInfo(std::string* pstInfo)
{
	char szInfo[256];
	sprintf(szInfo, "Item: Inst %d, Pool %d", m_GroundItemInstanceMap.size(), m_GroundItemInstancePool.GetCapacity());

	pstInfo->append(szInfo);
}

void CPythonItem::TGroundItemInstance::Clear()
{
	stOwnership = "";
	ThingInstance.Clear();
	CEffectManager::Instance().DestroyEffectInstance(dwEffectInstanceIndex);
}

void CPythonItem::TGroundItemInstance::__PlayDropSound(DWORD eItemType, const D3DXVECTOR3& c_rv3Pos)
{
	if (eItemType>=DROPSOUND_NUM)
		return;

	CSoundManager::Instance().PlaySound3D(c_rv3Pos.x, c_rv3Pos.y, c_rv3Pos.z, ms_astDropSoundFileName[eItemType].c_str());
}

bool CPythonItem::TGroundItemInstance::Update()
{
	if (bAnimEnded)
		return false;
	if (dwEndTime < CTimer::Instance().GetCurrentMillisecond())
	{
		ThingInstance.SetRotationQuaternion(qEnd);

		/*D3DXVECTOR3 v3Adjust = -v3Center;
		D3DXMATRIX mat;
		D3DXMatrixRotationYawPitchRoll(&mat,
		D3DXToRadian(rEnd.y),
		D3DXToRadian(rEnd.x),
		D3DXToRadian(rEnd.z));
		D3DXVec3TransformCoord(&v3Adjust,&v3Adjust,&mat);*/

		D3DXQUATERNION qAdjust(-v3Center.x, -v3Center.y, -v3Center.z, 0.0f);
		D3DXQUATERNION qc;
		D3DXQuaternionConjugate(&qc, &qEnd);
		D3DXQuaternionMultiply(&qAdjust,&qAdjust,&qEnd);
		D3DXQuaternionMultiply(&qAdjust,&qc,&qAdjust);

		ThingInstance.SetPosition(v3EndPosition.x+qAdjust.x,
			v3EndPosition.y+qAdjust.y,
			v3EndPosition.z+qAdjust.z);
		//ThingInstance.Update();
		bAnimEnded = true;

		__PlayDropSound(eDropSoundType, v3EndPosition);
	}
	else
	{
		const DWORD time = CTimer::Instance().GetCurrentMillisecond() - dwStartTime;
		const DWORD etime = dwEndTime - CTimer::Instance().GetCurrentMillisecond();
		const float rate = time * 1.0f / (dwEndTime - dwStartTime);

		D3DXVECTOR3 v3NewPosition=v3EndPosition;// = rate*(v3EndPosition - v3StartPosition) + v3StartPosition;
		v3NewPosition.z += 100-100*rate*(3*rate-2);//-100*(rate-1)*(3*rate+2);

		D3DXQUATERNION q;
		D3DXQuaternionRotationAxis(&q, &v3RotationAxis, etime * 0.03f *(-1+rate*(3*rate-2)));
		//ThingInstance.SetRotation(rEnd.y + etime*rStart.y, rEnd.x + etime*rStart.x, rEnd.z + etime*rStart.z);
		D3DXQuaternionMultiply(&q,&qEnd,&q);

		ThingInstance.SetRotationQuaternion(q);
		D3DXQUATERNION qAdjust(-v3Center.x, -v3Center.y, -v3Center.z, 0.0f);
		D3DXQUATERNION qc;
		D3DXQuaternionConjugate(&qc, &q);
		D3DXQuaternionMultiply(&qAdjust,&qAdjust,&q);
		D3DXQuaternionMultiply(&qAdjust,&qc,&qAdjust);

		ThingInstance.SetPosition(v3NewPosition.x+qAdjust.x,
			v3NewPosition.y+qAdjust.y,
			v3NewPosition.z+qAdjust.z);

		/*D3DXVECTOR3 v3Adjust = -v3Center;
		D3DXMATRIX mat;
		D3DXMatrixRotationYawPitchRoll(&mat,
		D3DXToRadian(rEnd.y + etime*rStart.y),
		D3DXToRadian(rEnd.x + etime*rStart.x),
		D3DXToRadian(rEnd.z + etime*rStart.z));

		D3DXVec3TransformCoord(&v3Adjust,&v3Adjust,&mat);
		//Tracef("%f %f %f\n",v3Adjust.x,v3Adjust.y,v3Adjust.z);
		v3NewPosition += v3Adjust;
		ThingInstance.SetPosition(v3NewPosition.x, v3NewPosition.y, v3NewPosition.z);*/
	}
	ThingInstance.Transform();
	ThingInstance.Deform();
	return !bAnimEnded;
}

void CPythonItem::Update(const POINT& c_rkPtMouse)
{
	auto itor = m_GroundItemInstanceMap.begin();
	for(; itor != m_GroundItemInstanceMap.end(); ++itor)
	{
		itor->second->Update();
	}

	m_dwPickedItemID=__Pick(c_rkPtMouse);
}

void CPythonItem::Render()
{
	CPythonGraphic::Instance().SetDiffuseOperation();
	auto itor = m_GroundItemInstanceMap.begin();
	for (; itor != m_GroundItemInstanceMap.end(); ++itor)
	{
		CGraphicThingInstance & rInstance = itor->second->ThingInstance;
		//rInstance.Update();
		rInstance.Render();
		rInstance.BlendRender();
	}
}

void CPythonItem::SetUseSoundFileName(DWORD eItemType, const std::string& c_rstFileName)
{
	if (eItemType>=USESOUND_NUM)
		return;

	//Tracenf("SetUseSoundFile %d : %s", eItemType, c_rstFileName.c_str());

	m_astUseSoundFileName[eItemType]=c_rstFileName;
}

void CPythonItem::SetDropSoundFileName(DWORD eItemType, const std::string& c_rstFileName) const
{
	if (eItemType>=DROPSOUND_NUM)
		return;

	Tracenf("SetDropSoundFile %d : %s", eItemType, c_rstFileName.c_str());

	SGroundItemInstance::ms_astDropSoundFileName[eItemType]=c_rstFileName;
}

void	CPythonItem::PlayUseSound(DWORD dwItemID) const
{
	//CItemManager& rkItemMgr=CItemManager::Instance();

	CItemData* pkItemData;
	if (!CItemManager::Instance().GetItemDataPointer(dwItemID, &pkItemData))
		return;

	const DWORD eItemType=__GetUseSoundType(*pkItemData);
	if (eItemType==USESOUND_NONE)
		return;
	if (eItemType>=USESOUND_NUM)
		return;

	CSoundManager::Instance().PlaySound2D(m_astUseSoundFileName[eItemType].c_str());
}

void	CPythonItem::PlayDropSound(DWORD dwItemID) const
{
	//CItemManager& rkItemMgr=CItemManager::Instance();

	CItemData* pkItemData;
	if (!CItemManager::Instance().GetItemDataPointer(dwItemID, &pkItemData))
		return;

	const DWORD eItemType=__GetDropSoundType(*pkItemData);
	if (eItemType>=DROPSOUND_NUM)
		return;

	CSoundManager::Instance().PlaySound2D(SGroundItemInstance::ms_astDropSoundFileName[eItemType].c_str());
}

void	CPythonItem::PlayUsePotionSound() const
{
	CSoundManager::Instance().PlaySound2D(m_astUseSoundFileName[USESOUND_POTION].c_str());
}

DWORD	CPythonItem::__GetDropSoundType(const CItemData& c_rkItemData) const
{
	switch (c_rkItemData.GetType())
	{
		case CItemData::ITEM_TYPE_WEAPON:
			switch (c_rkItemData.GetWeaponType())
			{
				case CItemData::WEAPON_BOW:
					return DROPSOUND_BOW;
#ifdef ENABLE_QUIVER_SYSTEM
				case CItemData::WEAPON_QUIVER:
#endif
				case CItemData::WEAPON_ARROW:
					return DROPSOUND_DEFAULT;
				default:
					return DROPSOUND_WEAPON;
			}
		case CItemData::ITEM_TYPE_ARMOR:
			switch (c_rkItemData.GetSubType())
			{
				case CItemData::ARMOR_NECK:
				case CItemData::ARMOR_EAR:
					return DROPSOUND_ACCESSORY;
				case CItemData::ARMOR_BODY:
					return DROPSOUND_ARMOR;
				default:
					return DROPSOUND_DEFAULT;
			}
		default:
			return DROPSOUND_DEFAULT;
	}
}

DWORD	CPythonItem::__GetUseSoundType(const CItemData& c_rkItemData) const
{
	switch (c_rkItemData.GetType())
	{
		case CItemData::ITEM_TYPE_WEAPON:
			switch (c_rkItemData.GetWeaponType())
			{
				case CItemData::WEAPON_BOW:
					return USESOUND_BOW;
#ifdef ENABLE_QUIVER_SYSTEM
				case CItemData::WEAPON_QUIVER:
#endif
				case CItemData::WEAPON_ARROW:
					return USESOUND_DEFAULT;
				default:
					return USESOUND_WEAPON;
			}
		case CItemData::ITEM_TYPE_ARMOR:
			switch (c_rkItemData.GetSubType())
			{
				case CItemData::ARMOR_NECK:
				case CItemData::ARMOR_EAR:
					return USESOUND_ACCESSORY;
				case CItemData::ARMOR_BODY:
					return USESOUND_ARMOR;
				default:
					return USESOUND_DEFAULT;
			}
		case CItemData::ITEM_TYPE_USE:
			switch (c_rkItemData.GetSubType())
			{
				case CItemData::USE_ABILITY_UP:
					return USESOUND_POTION;
				case CItemData::USE_POTION:
					return USESOUND_NONE;
				case CItemData::USE_TALISMAN:
					return USESOUND_PORTAL;
				default:
					return USESOUND_DEFAULT;
			}
		default:
			return USESOUND_DEFAULT;
	}
}

void CPythonItem::CreateItem(DWORD dwVirtualID, DWORD dwVirtualNumber, float x, float y, float z, bool bDrop
#ifdef ENABLE_ITEM_GROUND_EX
	, DWORD count
	, std::array<TPlayerItemAttribute, ITEM_ATTRIBUTE_SLOT_MAX_NUM> attrs
	, std::array<long, ITEM_SOCKET_SLOT_MAX_NUM> sockets
#endif
)
{
	CItemData * pItemData;
	if (!CItemManager::Instance().GetItemDataPointer(dwVirtualNumber, &pItemData))
		return;

	CGraphicThing* pItemModel = pItemData->GetDropModelThing();

	TGroundItemInstance *	pGroundItemInstance = m_GroundItemInstancePool.Alloc();
	pGroundItemInstance->dwVirtualNumber = dwVirtualNumber;
#ifdef ENABLE_ITEM_GROUND_EX
	pGroundItemInstance->count = count;
	pGroundItemInstance->attrs = attrs;
	pGroundItemInstance->sockets = sockets;
#endif

	bool bStabGround = false;

	if (bDrop)
	{
		z = CPythonBackground::Instance().GetHeight(x, y) + 10.0f;
		bStabGround = false;
		pGroundItemInstance->bAnimEnded = false;
	}
	else
	{
		pGroundItemInstance->bAnimEnded = true;
	}

	{
		// attaching effect
		CEffectManager & rem =CEffectManager::Instance();
		pGroundItemInstance->dwEffectInstanceIndex =
		rem.CreateEffect(m_dwDropItemEffectID, D3DXVECTOR3(x, -y, z), D3DXVECTOR3(0,0,0));

		pGroundItemInstance->eDropSoundType=__GetDropSoundType(*pItemData);
	}

	D3DXVECTOR3 normal;
	if (!CPythonBackground::Instance().GetNormal(int(x),int(y),&normal))
		normal = D3DXVECTOR3(0.0f,0.0f,1.0f);

	pGroundItemInstance->ThingInstance.Clear();
	pGroundItemInstance->ThingInstance.ReserveModelThing(1);
	pGroundItemInstance->ThingInstance.ReserveModelInstance(1);
	pGroundItemInstance->ThingInstance.RegisterModelThing(0, pItemModel);
	pGroundItemInstance->ThingInstance.SetModelInstance(0, 0, 0);
	if (bDrop)
	{
		pGroundItemInstance->v3EndPosition = D3DXVECTOR3(x,-y,z);
		pGroundItemInstance->ThingInstance.SetPosition(0,0,0);
	}
	else
		pGroundItemInstance->ThingInstance.SetPosition(x, -y, z);

	pGroundItemInstance->ThingInstance.Update();
	pGroundItemInstance->ThingInstance.Transform();
	pGroundItemInstance->ThingInstance.Deform();

	if (bDrop)
	{
		D3DXVECTOR3 vMin, vMax;
		pGroundItemInstance->ThingInstance.GetBoundBox(&vMin,&vMax);
		pGroundItemInstance->v3Center = (vMin + vMax) * 0.5f;

		std::pair<float,int> f[3] =
			{
				std::make_pair(vMax.x - vMin.x,0),
				std::make_pair(vMax.y - vMin.y,1),
				std::make_pair(vMax.z - vMin.z,2)
			};

		std::sort(f,f+3);

		D3DXVECTOR3 rEnd;

		if (bStabGround)
		{
			if (f[2].second == 0) // axis x
			{
				rEnd.y = 90.0f + frandom(-15.0f, 15.0f);
				rEnd.x = frandom(0.0f, 360.0f);
				rEnd.z = frandom(-15.0f, 15.0f);
			}
			else if (f[2].second == 1) // axis y
			{
				rEnd.y = frandom(0.0f, 360.0f);
				rEnd.x = frandom(-15.0f, 15.0f);
				rEnd.z = 180.0f + frandom(-15.0f, 15.0f);
			}
			else // axis z
			{
				rEnd.y = 180.0f + frandom(-15.0f, 15.0f);
				rEnd.x = 0.0f+frandom(-15.0f, 15.0f);
				rEnd.z = frandom(0.0f, 360.0f);
			}
		}
		else
		{
			if (f[0].second == 0)
			{
				// y,z = by normal
				pGroundItemInstance->qEnd =
					RotationArc(
						D3DXVECTOR3(
						((float)(random()%2))*2-1+frandom(-0.1f,0.1f),
						0+frandom(-0.1f,0.1f),
						0+frandom(-0.1f,0.1f)),
						D3DXVECTOR3(0,0,1)/*normal*/);
			}
			else if (f[0].second == 1)
			{
				pGroundItemInstance->qEnd =
					RotationArc(
						D3DXVECTOR3(
							0+frandom(-0.1f,0.1f),
							((float)(random()%2))*2-1+frandom(-0.1f,0.1f),
							0+frandom(-0.1f,0.1f)),
						D3DXVECTOR3(0,0,1)/*normal*/);
			}
			else
			{
				pGroundItemInstance->qEnd =
					RotationArc(
					D3DXVECTOR3(
					0+frandom(-0.1f,0.1f),
					0+frandom(-0.1f,0.1f),
					((float)(random()%2))*2-1+frandom(-0.1f,0.1f)),
					D3DXVECTOR3(0,0,1)/*normal*/);
			}
		}

		const float rot = frandom(0, 2*3.1415926535f);
		D3DXQUATERNION q(0,0,cosf(rot),sinf(rot));
		D3DXQuaternionMultiply(&pGroundItemInstance->qEnd, &pGroundItemInstance->qEnd, &q);
		q = RotationArc(D3DXVECTOR3(0,0,1),normal);
		D3DXQuaternionMultiply(&pGroundItemInstance->qEnd, &pGroundItemInstance->qEnd, &q);

		pGroundItemInstance->dwStartTime = CTimer::Instance().GetCurrentMillisecond();
		pGroundItemInstance->dwEndTime = pGroundItemInstance->dwStartTime+300;
		pGroundItemInstance->v3RotationAxis.x = sinf(rot+0);
		pGroundItemInstance->v3RotationAxis.y = cosf(rot+0);
		pGroundItemInstance->v3RotationAxis.z = 0;

		D3DXVECTOR3 v3Adjust = -pGroundItemInstance->v3Center;
		D3DXMATRIX mat;
		D3DXMatrixRotationQuaternion(&mat, &pGroundItemInstance->qEnd);

		D3DXVec3TransformCoord(&v3Adjust,&v3Adjust,&mat);
	}

	pGroundItemInstance->ThingInstance.Show();

	m_GroundItemInstanceMap.emplace(dwVirtualID, pGroundItemInstance);

	std::string itemName = pItemData->GetName();

#ifdef ENABLE_ITEM_GROUND_EX
	if (dwVirtualNumber == 50300 || dwVirtualNumber == 70037) {
		auto skillVnum = sockets[0];
		if (skillVnum) {
			CPythonSkill::SSkillData* c_pSkillData{};
			if (CPythonSkill::Instance().GetSkillData(skillVnum, &c_pSkillData))
				itemName = fmt::format("{} {}", c_pSkillData->strName, itemName);
		}
	}

	if (count > 1 && dwVirtualNumber >= 10) // not yang and currencies
		itemName = fmt::format("{} ({}x)", itemName, NumberToK(count));
	else if (dwVirtualNumber < 10) // yang and currencies
		itemName = fmt::format("{} {}", NumberToK(count), itemName);
#endif

	CPythonTextTail& rkTextTail=CPythonTextTail::Instance();
	rkTextTail.RegisterItemTextTail(
		dwVirtualID,
		itemName.c_str(),
		&pGroundItemInstance->ThingInstance);
}

void CPythonItem::SetOwnership(DWORD dwVID, const char * c_pszName)
{
	const auto itor = m_GroundItemInstanceMap.find(dwVID);

	if (m_GroundItemInstanceMap.end() == itor)
		return;

	TGroundItemInstance * pGroundItemInstance = itor->second;
	pGroundItemInstance->stOwnership.assign(c_pszName);

	CPythonTextTail& rkTextTail = CPythonTextTail::Instance();
	rkTextTail.SetItemTextTailOwner(dwVID, c_pszName);
}

bool CPythonItem::GetOwnership(DWORD dwVID, const char ** c_pszName)
{
	const auto itor = m_GroundItemInstanceMap.find(dwVID);

	if (m_GroundItemInstanceMap.end() == itor)
		return false;

	const TGroundItemInstance * pGroundItemInstance = itor->second;
	*c_pszName = pGroundItemInstance->stOwnership.c_str();

	return true;
}

void CPythonItem::DeleteAllItems()
{
	CPythonTextTail& rkTextTail=CPythonTextTail::Instance();

	TGroundItemInstanceMap::iterator i;
	for (i= m_GroundItemInstanceMap.begin(); i!=m_GroundItemInstanceMap.end(); ++i)
	{
		TGroundItemInstance* pGroundItemInst=i->second;
		rkTextTail.DeleteItemTextTail(i->first);
		pGroundItemInst->Clear();
		m_GroundItemInstancePool.Free(pGroundItemInst);
	}
	m_GroundItemInstanceMap.clear();
}

void CPythonItem::DeleteItem(DWORD dwVirtualID)
{
	const auto itor = m_GroundItemInstanceMap.find(dwVirtualID);
	if (m_GroundItemInstanceMap.end() == itor)
		return;

	TGroundItemInstance * pGroundItemInstance = itor->second;
	pGroundItemInstance->Clear();
	m_GroundItemInstancePool.Free(pGroundItemInstance);
	m_GroundItemInstanceMap.erase(itor);

	// Text Tail
	CPythonTextTail::Instance().DeleteItemTextTail(dwVirtualID);
}

bool CPythonItem::GetCloseMoney(const TPixelPosition & c_rPixelPosition, DWORD * pdwItemID, DWORD dwDistance)
{
	DWORD dwCloseItemID = 0;
	DWORD dwCloseItemDistance = 1000 * 1000;

	TGroundItemInstanceMap::iterator i;
	for (i = m_GroundItemInstanceMap.begin(); i != m_GroundItemInstanceMap.end(); ++i)
	{
		const TGroundItemInstance * pInstance = i->second;

		if (pInstance->dwVirtualNumber!=VNUM_MONEY)
			continue;

		const DWORD dwxDistance = DWORD(c_rPixelPosition.x-pInstance->v3EndPosition.x);
		const DWORD dwyDistance = DWORD(c_rPixelPosition.y-(-pInstance->v3EndPosition.y));
		const DWORD dwDistance = DWORD(dwxDistance*dwxDistance + dwyDistance*dwyDistance);

		if (dwxDistance*dwxDistance + dwyDistance*dwyDistance < dwCloseItemDistance)
		{
			dwCloseItemID = i->first;
			dwCloseItemDistance = dwDistance;
		}
	}

	if (dwCloseItemDistance>float(dwDistance)*float(dwDistance))
		return false;

	*pdwItemID=dwCloseItemID;

	return true;
}

bool CPythonItem::GetCloseItem(const TPixelPosition & c_rPixelPosition, DWORD * pdwItemID, DWORD dwDistance)
{
	DWORD dwCloseItemID = 0;
	DWORD dwCloseItemDistance = 1000 * 1000;
#ifdef ENABLE_MULTI_ITEM_PICK
	vecMultiItemPick.clear();
#endif

	TGroundItemInstanceMap::iterator i;
	for (i = m_GroundItemInstanceMap.begin(); i != m_GroundItemInstanceMap.end(); ++i)
	{
		const TGroundItemInstance * pInstance = i->second;

		const DWORD dwxDistance = DWORD(c_rPixelPosition.x)-DWORD(pInstance->v3EndPosition.x); // @fixme022
		const DWORD dwyDistance = DWORD(c_rPixelPosition.y)-DWORD(-pInstance->v3EndPosition.y); // @fixme022
		const DWORD dwDistance = dwxDistance*dwxDistance + dwyDistance*dwyDistance;

#if defined(__BL_OFFICIAL_LOOT_FILTER__)
		if (!CanLoot(i))
			continue;
#endif
		if (dwDistance < dwCloseItemDistance)
		{
			dwCloseItemID = i->first;
			dwCloseItemDistance = dwDistance;
		}
#ifdef ENABLE_MULTI_ITEM_PICK
		vecMultiItemPick.emplace_back(i->first);
#endif
	}

	if (dwCloseItemDistance>float(dwDistance)*float(dwDistance))
		return false;

	*pdwItemID=dwCloseItemID;

	return true;
}

BOOL CPythonItem::GetGroundItemPosition(DWORD dwVirtualID, TPixelPosition * pPosition)
{
	const auto itor = m_GroundItemInstanceMap.find(dwVirtualID);
	if (m_GroundItemInstanceMap.end() == itor)
		return FALSE;

	const TGroundItemInstance * pInstance = itor->second;

	const D3DXVECTOR3& rkD3DVct3=pInstance->ThingInstance.GetPosition();

	pPosition->x=+rkD3DVct3.x;
	pPosition->y=-rkD3DVct3.y;
	pPosition->z=+rkD3DVct3.z;

	return TRUE;
}

DWORD CPythonItem::__Pick(const POINT& c_rkPtMouse)
{
	float fu, fv, ft;

	auto itor = m_GroundItemInstanceMap.begin();
	for (; itor != m_GroundItemInstanceMap.end(); ++itor)
	{
		TGroundItemInstance * pInstance = itor->second;

		if (pInstance->ThingInstance.Intersect(&fu, &fv, &ft))
		{
			return itor->first;
		}
	}

	CPythonTextTail& rkTextTailMgr=CPythonTextTail::Instance();
	return rkTextTailMgr.Pick(c_rkPtMouse.x, c_rkPtMouse.y);
}

bool CPythonItem::GetPickedItemID(DWORD* pdwPickedItemID) const
{
	if (INVALID_ID==m_dwPickedItemID)
		return false;

	*pdwPickedItemID=m_dwPickedItemID;
	return true;
}

DWORD CPythonItem::GetVirtualNumberOfGroundItem(DWORD dwVID)
{
	const auto itor = m_GroundItemInstanceMap.find(dwVID);

	if (itor == m_GroundItemInstanceMap.end())
		return 0;
	else
		return itor->second->dwVirtualNumber;
}

void CPythonItem::BuildNoGradeNameData(int iType) const
{
}

DWORD CPythonItem::GetNoGradeNameDataCount() const
{
	return m_NoGradeNameItemData.size();
}

CItemData * CPythonItem::GetNoGradeNameDataPtr(DWORD dwIndex) const
{
	if (dwIndex >= m_NoGradeNameItemData.size())
		return nullptr;

	return m_NoGradeNameItemData[dwIndex];
}

void CPythonItem::Destroy()
{
	DeleteAllItems();
	m_GroundItemInstancePool.Clear();
}

void CPythonItem::Create()
{
	CEffectManager::Instance().RegisterEffect2("d:/ymir work/effect/etc/dropitem/dropitem.mse", &m_dwDropItemEffectID);
}

CPythonItem::CPythonItem()
{
	m_GroundItemInstancePool.SetName("CDynamicPool<TGroundItemInstance>");
	m_dwPickedItemID = INVALID_ID;
}

CPythonItem::~CPythonItem()
{
	assert(m_GroundItemInstanceMap.empty());
}

#if defined(__BL_OFFICIAL_LOOT_FILTER__)
#include "PythonCharacterManager.h"

void CPythonItem::SaveLootingSettings(const std::string_view jsonData)
{
	mLootingData.Parse(jsonData.data());
	if (mLootingData.HasParseError()) {
		// TraceError("SaveLootingSettings LOAD ERROR %s", jsonData.data());
		return;
	}
	// TraceError("SaveLootingSettings LOAD OK");
}

bool JsonGetDataBool(const rapidjson::Value& data, std::string_view name1, std::string_view name2, bool defaultValue) {
	const auto value1 = data.FindMember(name1.data());
	if (value1 != data.MemberEnd() && value1->value.IsObject()) {
		const auto value2 = value1->value.FindMember(name2.data());
		if (value2 != value1->value.MemberEnd() && value2->value.IsBool()) {
			return value2->value.GetBool();
		}
	}
	return defaultValue;
}

bool JsonGetDataBool(const rapidjson::Value& data, std::string_view name1, std::string_view name2, std::string_view name3, bool defaultValue) {
	const auto value1 = data.FindMember(name1.data());
	if (value1 != data.MemberEnd() && value1->value.IsObject()) {
		const auto value2 = value1->value.FindMember(name2.data());
		if (value2 != value1->value.MemberEnd() && value2->value.IsObject()) {
			const auto value3 = value2->value.FindMember(name3.data());
			if (value3 != value2->value.MemberEnd() && value3->value.IsBool()) {
				return value3->value.GetBool();
			}
		}
	}
	return defaultValue;
}

int JsonGetDataInt(const rapidjson::Value& data, std::string_view name1, std::string_view name2, int defaultValue) {
	const auto value1 = data.FindMember(name1.data());
	if (value1 != data.MemberEnd() && value1->value.IsObject()) {
		const auto value2 = value1->value.FindMember(name2.data());
		if (value2 != value1->value.MemberEnd() && value2->value.IsInt()) {
			return value2->value.GetInt();
		}
	}
	return defaultValue;
}

int JsonGetDataInt(const rapidjson::Value& data, std::string_view name1, std::string_view name2, std::string_view name3, int defaultValue) {
	const auto value1 = data.FindMember(name1.data());
	if (value1 != data.MemberEnd() && value1->value.IsObject()) {
		const auto value2 = value1->value.FindMember(name2.data());
		if (value2 != value1->value.MemberEnd() && value2->value.IsObject()) {
			const auto value3 = value2->value.FindMember(name3.data());
			if (value3 != value2->value.MemberEnd() && value3->value.IsInt()) {
				return value3->value.GetInt();
			}
		}
	}
	return defaultValue;
}

bool LootCanPickWeapon(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "weapon", "onoff", true))
		return false;

	const auto refineLevel = pItemData->GetRealRefine();
	if (refineLevel < JsonGetDataInt(mLootingData, "weapon", "refine_min", 0))
		return false;
	else if (refineLevel > JsonGetDataInt(mLootingData, "weapon", "refine_max", 255))
		return false;

	const auto wearingLevel = pItemData->GetLevelLimit();
	if (wearingLevel < JsonGetDataInt(mLootingData, "weapon", "wearing_level_min", 0))
		return false;
	else if (wearingLevel > JsonGetDataInt(mLootingData, "weapon", "wearing_level_max", 255))
		return false;

	if (JsonGetDataBool(mLootingData, "weapon", "select", "warrior", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WARRIOR))
		return true;
	else if (JsonGetDataBool(mLootingData, "weapon", "select", "assassin", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_ASSASSIN))
		return true;
	else if (JsonGetDataBool(mLootingData, "weapon", "select", "sura", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SURA))
		return true;
	else if (JsonGetDataBool(mLootingData, "weapon", "select", "shaman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SHAMAN))
		return true;
	#ifdef ENABLE_WOLFMAN_CHARACTER
	else if (JsonGetDataBool(mLootingData, "weapon", "select", "wolfman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WOLFMAN))
		return true;
	#endif
	return false;
}

bool LootCanPickArmor(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "armor", "onoff", true))
		return false;

	const auto refineLevel = pItemData->GetRealRefine();
	if (refineLevel < JsonGetDataInt(mLootingData, "armor", "refine_min", 0))
		return false;
	else if (refineLevel > JsonGetDataInt(mLootingData, "armor", "refine_max", 255))
		return false;

	const auto wearingLevel = pItemData->GetLevelLimit();
	if (wearingLevel < JsonGetDataInt(mLootingData, "armor", "wearing_level_min", 0))
		return false;
	else if (wearingLevel > JsonGetDataInt(mLootingData, "armor", "wearing_level_max", 255))
		return false;

	if (JsonGetDataBool(mLootingData, "armor", "select", "warrior", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WARRIOR))
		return true;
	else if (JsonGetDataBool(mLootingData, "armor", "select", "assassin", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_ASSASSIN))
		return true;
	else if (JsonGetDataBool(mLootingData, "armor", "select", "sura", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SURA))
		return true;
	else if (JsonGetDataBool(mLootingData, "armor", "select", "shaman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SHAMAN))
		return true;
	#ifdef ENABLE_WOLFMAN_CHARACTER
	else if (JsonGetDataBool(mLootingData, "armor", "select", "wolfman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WOLFMAN))
		return true;
	#endif
	return false;
}

bool LootCanPickHead(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "head", "onoff", true))
		return false;

	const auto refineLevel = pItemData->GetRealRefine();
	if (refineLevel < JsonGetDataInt(mLootingData, "head", "refine_min", 0))
		return false;
	else if (refineLevel > JsonGetDataInt(mLootingData, "head", "refine_max", 255))
		return false;

	const auto wearingLevel = pItemData->GetLevelLimit();
	if (wearingLevel < JsonGetDataInt(mLootingData, "head", "wearing_level_min", 0))
		return false;
	else if (wearingLevel > JsonGetDataInt(mLootingData, "head", "wearing_level_max", 255))
		return false;

	if (JsonGetDataBool(mLootingData, "head", "select", "warrior", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WARRIOR))
		return true;
	else if (JsonGetDataBool(mLootingData, "head", "select", "assassin", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_ASSASSIN))
		return true;
	else if (JsonGetDataBool(mLootingData, "head", "select", "sura", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SURA))
		return true;
	else if (JsonGetDataBool(mLootingData, "head", "select", "shaman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_SHAMAN))
		return true;
	#ifdef ENABLE_WOLFMAN_CHARACTER
	else if (JsonGetDataBool(mLootingData, "head", "select", "wolfman", true) && !pItemData->IsAntiFlag(CItemData::ITEM_ANTIFLAG_WOLFMAN))
		return true;
	#endif
	return false;
}

bool LootCanPickCommon(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "common", "onoff", true))
		return false;

	const auto refineLevel = pItemData->GetRealRefine();
	if (refineLevel < JsonGetDataInt(mLootingData, "common", "refine_min", 0))
		return false;
	else if (refineLevel > JsonGetDataInt(mLootingData, "common", "refine_max", 255))
		return false;

	const auto wearingLevel = pItemData->GetLevelLimit();
	if (wearingLevel < JsonGetDataInt(mLootingData, "common", "wearing_level_min", 0))
		return false;
	else if (wearingLevel > JsonGetDataInt(mLootingData, "common", "wearing_level_max", 255))
		return false;

	switch (pItemData->GetType())
	{
	case CItemData::ITEM_TYPE_BELT:
		return JsonGetDataBool(mLootingData, "common", "select", "belt", true);
	case CItemData::ITEM_TYPE_ROD:
		return JsonGetDataBool(mLootingData, "common", "select", "rod", true);
	case CItemData::ITEM_TYPE_PICK:
		return JsonGetDataBool(mLootingData, "common", "select", "pick", true);
	case CItemData::ITEM_TYPE_ARMOR:
		switch (pItemData->GetSubType())
		{
		case CItemData::ARMOR_FOOTS:
			return JsonGetDataBool(mLootingData, "common", "select", "foots", true);
		case CItemData::ARMOR_WRIST:
			return JsonGetDataBool(mLootingData, "common", "select", "wrist", true);
		case CItemData::ARMOR_NECK:
			return JsonGetDataBool(mLootingData, "common", "select", "neck", true);
		case CItemData::ARMOR_EAR:
			return JsonGetDataBool(mLootingData, "common", "select", "ear", true);
		case CItemData::ARMOR_SHIELD:
			return JsonGetDataBool(mLootingData, "common", "select", "shield", true);
		#ifdef ENABLE_PENDANT_SYSTEM
		case CItemData::ARMOR_PENDANT:
			return JsonGetDataBool(mLootingData, "common", "select", "pendant", true);
		#endif
		#ifdef ENABLE_GLOVE_SYSTEM
		case CItemData::ARMOR_GLOVE:
			return JsonGetDataBool(mLootingData, "common", "select", "glove", true);
		#endif
		default:
			return true;
		}
	default:
		return true;
	}
}

bool LootCanPickMountPet(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "mount_pet", "onoff", true))
		return false;

	if (pItemData->GetType() == CItemData::ITEM_TYPE_COSTUME && pItemData->GetSubType() == CItemData::COSTUME_MOUNT)
		return JsonGetDataBool(mLootingData, "mount_pet", "select", "mount", true);
	else if (pItemData->GetType() == CItemData::ITEM_TYPE_PET)
	{
		switch (pItemData->GetSubType())
		{
		case CItemData::PET_EGG:
			return JsonGetDataBool(mLootingData, "mount_pet", "select", "egg", true);
		case CItemData::PET_PAY:
			return JsonGetDataBool(mLootingData, "mount_pet", "select", "charged_pet", true);
		case CItemData::PET_UPBRINGING: // may be incorrect
			return JsonGetDataBool(mLootingData, "mount_pet", "select", "free_pet", true);
		}
	}
	return true;
}

bool LootCanPickCostume(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "costume", "onoff", true))
		return false;

	switch (pItemData->GetType()) {
	case CItemData::ITEM_TYPE_COSTUME:
		switch (pItemData->GetSubType()) {
		case CItemData::COSTUME_BODY:
			return JsonGetDataBool(mLootingData, "costume", "select", "armor", true);
		case CItemData::COSTUME_HAIR:
			return JsonGetDataBool(mLootingData, "costume", "select", "hair", true);
		#ifdef ENABLE_WEAPON_COSTUME_SYSTEM
		case CItemData::COSTUME_WEAPON:
			return JsonGetDataBool(mLootingData, "costume", "select", "weapon", true);
		#endif
		#ifdef ENABLE_ACCE_COSTUME_SYSTEM
		case CItemData::COSTUME_ACCE:
			return JsonGetDataBool(mLootingData, "costume", "select", "acce", true);
		#endif
		case CItemData::COSTUME_MOUNT:
			return LootCanPickMountPet(pItemData, mLootingData);
		default:
			return true;
		}
	default:
		return JsonGetDataBool(mLootingData, "costume", "select", "etc", true);
	}

	return true;
}

bool LootCanPickDS(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "ds", "onoff", true))
		return false;

	if (pItemData->IsCorDraconis())
		return JsonGetDataBool(mLootingData, "ds", "select", "ds", true);

	return JsonGetDataBool(mLootingData, "ds", "select", "etc", true);
}

bool LootCanPickUnique(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "unique", "onoff", true))
		return false;

	return true;
}

bool LootCanPickRefine(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "refine", "onoff", true))
		return false;

	switch (pItemData->GetType())
	{
	case CItemData::ITEM_TYPE_MATERIAL:
		switch (pItemData->GetSubType())
		{
		case CItemData::MATERIAL_LEATHER:
			return JsonGetDataBool(mLootingData, "refine", "select", "material", true);
		default:
			return JsonGetDataBool(mLootingData, "refine", "select", "etc", true);
		}
	case CItemData::ITEM_TYPE_METIN:
		return JsonGetDataBool(mLootingData, "refine", "select", "stone", true);
	default:
		return true;
	}
	return true;
}

bool LootCanPickPotion(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "potion", "onoff", true))
		return false;

	if (pItemData->IsHairDye())
		return JsonGetDataBool(mLootingData, "potion", "select", "hairdye", true);

	return JsonGetDataBool(mLootingData, "potion", "select", "ability", true);
}

bool LootCanPickFishMining(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "fish_mining", "onoff", true))
		return false;

	switch (pItemData->GetType())
	{
	case CItemData::ITEM_TYPE_USE:
		switch (pItemData->GetSubType())
		{
		case CItemData::USE_BAIT:
			return JsonGetDataBool(mLootingData, "fish_mining", "select", "food", true);
		case CItemData::USE_PUT_INTO_ACCESSORY_SOCKET:
			return JsonGetDataBool(mLootingData, "fish_mining", "select", "stone", true);
		default:
			break;
		}
	}

	return JsonGetDataBool(mLootingData, "fish_mining", "select", "etc", true);
}

bool LootCanPickSkillBook(const CItemData* pItemData, const rapidjson::Document& mLootingData, const CPythonItem::TGroundItemInstance* pkItemGround)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "skill_book", "onoff", true))
		return false;

	switch (pItemData->GetType())
	{
	case CItemData::ITEM_TYPE_SKILLBOOK:
	case CItemData::ITEM_TYPE_SKILLFORGET:
	{
		auto skillVnum = 0;
		#ifdef ENABLE_ITEM_GROUND_EX
		if (pItemData->GetIndex() == 50300 || pItemData->GetType() == CItemData::ITEM_TYPE_SKILLFORGET)
			skillVnum = pkItemGround->sockets[0];
		else
			skillVnum = pItemData->GetValue(0);
		#else
		skillVnum = pItemData->GetValue(0);
		#endif
		if (!skillVnum)
			return true;

		auto job = NRaceData::JOB_MAX_NUM;
		if (skillVnum < 30)
			job = NRaceData::JOB_WARRIOR;
		else if (skillVnum < 60)
			job = NRaceData::JOB_ASSASSIN;
		else if (skillVnum < 90)
			job = NRaceData::JOB_SURA;
		else if (skillVnum < 120)
			job = NRaceData::JOB_SHAMAN;
		#ifdef ENABLE_WOLFMAN_CHARACTER
		else if (skillVnum >= 170 && skillVnum < 180)
			job = NRaceData::JOB_WOLFMAN;
		#endif

		switch (job)
		{
		case (NRaceData::JOB_WARRIOR):
			return JsonGetDataBool(mLootingData, "skill_book", "select", "warrior", true);
		case (NRaceData::JOB_ASSASSIN):
			return JsonGetDataBool(mLootingData, "skill_book", "select", "assassin", true);
		case (NRaceData::JOB_SURA):
			return JsonGetDataBool(mLootingData, "skill_book", "select", "sura", true);
		case (NRaceData::JOB_SHAMAN):
			return JsonGetDataBool(mLootingData, "skill_book", "select", "shaman", true);
		#if defined(ENABLE_WOLFMAN_CHARACTER)
		case (NRaceData::JOB_WOLFMAN):
			return JsonGetDataBool(mLootingData, "skill_book", "select", "wolfman", true);
		#endif
		default:
			return true;
		}
	}
	default:
		return JsonGetDataBool(mLootingData, "skill_book", "select", "public", true);
	}

	return true;
}

bool LootCanPickEtc(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "etc", "onoff", true))
		return false;

	if (pItemData->GetType() == CItemData::ITEM_TYPE_GIFTBOX)
	{
		if (pItemData->IsCorDraconis())
			return LootCanPickDS(pItemData, mLootingData);
		else
			return JsonGetDataBool(mLootingData, "etc", "select", "giftbox", true);
	}
	else if (pItemData->GetType() == CItemData::ITEM_TYPE_POLYMORPH)
		return JsonGetDataBool(mLootingData, "etc", "select", "polymorph", true);
	else if (pItemData->IsArrow())
		return JsonGetDataBool(mLootingData, "etc", "select", "weapon_arrow", true);
	else if (pItemData->IsMarriage())
		return JsonGetDataBool(mLootingData, "etc", "select", "marriage", true);
	else if (pItemData->IsParty())
		return JsonGetDataBool(mLootingData, "etc", "select", "party", true);
	else if (pItemData->IsCraft())
		return JsonGetDataBool(mLootingData, "etc", "select", "recipe", true);
	else if (pItemData->IsScroll())
		return JsonGetDataBool(mLootingData, "etc", "select", "seal", true);

	return true;
}

bool LootCanPickEvent(const CItemData* pItemData, const rapidjson::Document& mLootingData)
{
	if (!pItemData)
		return false;

	if (!JsonGetDataBool(mLootingData, "event", "onoff", true))
		return false;

	return true;
}

bool CPythonItem::CanLoot(const TGroundItemInstanceMap::iterator& item) const
{
	if (!mLootingData.IsObject() || mLootingData.ObjectEmpty())
		return true;

	CItemData* pItemData{};
	if (!CItemManager::Instance().GetItemDataPointer(CPythonItem::Instance().GetVirtualNumberOfGroundItem(item->first), &pItemData))
		return false;

	const CInstanceBase* pCharacterInstance = CPythonCharacterManager::Instance().GetSelectedInstancePtr();
	if (!pCharacterInstance)
		return false;

	auto CanPickup = [&]() -> bool {
		// checks by vnum
		if (pItemData->IsHairDye())
			return LootCanPickPotion(pItemData, mLootingData);
		else if (pItemData->IsSkillBook())
			return LootCanPickSkillBook(pItemData, mLootingData, item->second);
		else if (pItemData->IsClam())
			return LootCanPickFishMining(pItemData, mLootingData);
		else if (pItemData->IsCorDraconis())
			return LootCanPickDS(pItemData, mLootingData);
		else if (pItemData->IsMarriage())
			return LootCanPickEtc(pItemData, mLootingData);

		// checks by item type
		switch (pItemData->GetType()) {
		case CItemData::ITEM_TYPE_WEAPON:
			if (pItemData->IsArrow())
				return LootCanPickEtc(pItemData, mLootingData);
			else
				return LootCanPickWeapon(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_ARMOR:
			switch (pItemData->GetSubType()) {
			case CItemData::ARMOR_BODY:
				return LootCanPickArmor(pItemData, mLootingData);
			case CItemData::ARMOR_HEAD:
				return LootCanPickHead(pItemData, mLootingData);
			default:
				return LootCanPickCommon(pItemData, mLootingData);
			}
		case CItemData::ITEM_TYPE_BELT:
		case CItemData::ITEM_TYPE_ROD:
		case CItemData::ITEM_TYPE_PICK:
			return LootCanPickCommon(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_COSTUME:
			return LootCanPickCostume(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_DS:
			return LootCanPickDS(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_UNIQUE:
			return LootCanPickUnique(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_SKILLBOOK:
		case CItemData::ITEM_TYPE_SKILLFORGET:
			return LootCanPickSkillBook(pItemData, mLootingData, item->second);
		case CItemData::ITEM_TYPE_MATERIAL:
		case CItemData::ITEM_TYPE_METIN:
			return LootCanPickRefine(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_USE:
			switch (pItemData->GetSubType()) {
			case CItemData::USE_POTION:
			case CItemData::USE_ABILITY_UP:
			case CItemData::USE_POTION_NODELAY:
			case CItemData::USE_POTION_CONTINUE:
				return LootCanPickPotion(pItemData, mLootingData);
			case CItemData::USE_BAIT:
			case CItemData::USE_PUT_INTO_ACCESSORY_SOCKET:
				return LootCanPickFishMining(pItemData, mLootingData);
			default:
				return LootCanPickEtc(pItemData, mLootingData);
			}
		case CItemData::ITEM_TYPE_PET:
			return LootCanPickMountPet(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_GIFTBOX:
		case CItemData::ITEM_TYPE_POLYMORPH:
			return LootCanPickEtc(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_FISH:
			return LootCanPickFishMining(pItemData, mLootingData);
		case CItemData::ITEM_TYPE_RESOURCE:
			switch (pItemData->GetSubType()) {
			case CItemData::RESOURCE_BLOOD_PEARL:
			case CItemData::RESOURCE_BLUE_PEARL:
			case CItemData::RESOURCE_WHITE_PEARL:
				return LootCanPickFishMining(pItemData, mLootingData);
			default:
				return true;
			}
		default:
			break;
		}

		return true;
	};

	const auto isPickable = CanPickup();

	return isPickable;
}
#endif

