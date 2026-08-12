#include "StdAfx.h"
#include "../eterLib/ResourceManager.h"

#include "ItemData.h"

CDynamicPool<CItemData>		CItemData::ms_kPool;
CItemData::TLocaleNameProvider CItemData::ms_pfnLocaleName = nullptr;

void CItemData::SetLocaleNameProvider(TLocaleNameProvider pfnProvider)
{
	ms_pfnLocaleName = pfnProvider;
}

extern DWORD GetDefaultCodePage();

CItemData* CItemData::New()
{
	return ms_kPool.Alloc();
}

void CItemData::Delete(CItemData* pkItemData)
{
	pkItemData->Clear();
	ms_kPool.Free(pkItemData);
}

void CItemData::DestroySystem()
{
	ms_kPool.Destroy();
}

CGraphicThing * CItemData::GetModelThing() const
{
	return m_pModelThing;
}

CGraphicThing * CItemData::GetSubModelThing() const
{
	if (m_pSubModelThing)
		return m_pSubModelThing;
	else
		return m_pModelThing;
}

CGraphicThing * CItemData::GetDropModelThing() const
{
	return m_pDropModelThing;
}

CGraphicSubImage * CItemData::GetIconImage()
{
	if(m_pIconImage == nullptr && m_strIconFileName.empty() == false)
		__SetIconImage(m_strIconFileName.c_str());
	return m_pIconImage;
}

DWORD CItemData::GetLODModelThingCount() const
{
	return m_pLODModelThingVector.size();
}

BOOL CItemData::GetLODModelThingPointer(DWORD dwIndex, CGraphicThing ** ppModelThing) const
{
	if (dwIndex >= m_pLODModelThingVector.size())
		return FALSE;

	*ppModelThing = m_pLODModelThingVector[dwIndex];

	return TRUE;
}

DWORD CItemData::GetAttachingDataCount() const
{
	return m_AttachingDataVector.size();
}

BOOL CItemData::GetCollisionDataPointer(DWORD dwIndex, const NRaceData::TAttachingData ** c_ppAttachingData) const
{
	if (dwIndex >= GetAttachingDataCount())
		return FALSE;

	if (NRaceData::ATTACHING_DATA_TYPE_COLLISION_DATA != m_AttachingDataVector[dwIndex].dwType)
		return FALSE;

	*c_ppAttachingData = &m_AttachingDataVector[dwIndex];
	return TRUE;
}

BOOL CItemData::GetAttachingDataPointer(DWORD dwIndex, const NRaceData::TAttachingData ** c_ppAttachingData) const
{
	if (dwIndex >= GetAttachingDataCount())
		return FALSE;

	*c_ppAttachingData = &m_AttachingDataVector[dwIndex];
	return TRUE;
}

void CItemData::SetSummary(const std::string& c_rstSumm)
{
	m_strSummary=c_rstSumm;
}

void CItemData::SetDescription(const std::string& c_rstDesc)
{
	m_strDescription=c_rstDesc;
}

void CItemData::SetDefaultItemData(const char * c_szIconFileName, const char * c_szModelFileName)
{
	if(c_szModelFileName)
	{
		m_strModelFileName = c_szModelFileName;
		m_strDropModelFileName = c_szModelFileName;
	}
	else
	{
		m_strModelFileName = "";
		m_strDropModelFileName = "d:/ymir work/item/etc/item_bag.gr2";
	}
	m_strIconFileName = c_szIconFileName;

	m_strSubModelFileName = "";
	m_strDescription = "";
	m_strSummary = "";
	memset(m_ItemTable.alSockets, 0, sizeof(m_ItemTable.alSockets));

	__LoadFiles();
}

void CItemData::__LoadFiles()
{
	// Model File Name
	if (!m_strModelFileName.empty())
		m_pModelThing = (CGraphicThing *)CResourceManager::Instance().GetResourcePointer(m_strModelFileName.c_str());

	if (!m_strSubModelFileName.empty())
		m_pSubModelThing = (CGraphicThing *)CResourceManager::Instance().GetResourcePointer(m_strSubModelFileName.c_str());

	if (!m_strDropModelFileName.empty())
		m_pDropModelThing = (CGraphicThing *)CResourceManager::Instance().GetResourcePointer(m_strDropModelFileName.c_str());

	if (!m_strLODModelFileNameVector.empty())
	{
		m_pLODModelThingVector.clear();
		m_pLODModelThingVector.resize(m_strLODModelFileNameVector.size());

		for (DWORD i = 0; i < m_strLODModelFileNameVector.size(); ++i)
		{
			const std::string & c_rstrLODModelFileName = m_strLODModelFileNameVector[i];
			m_pLODModelThingVector[i] = (CGraphicThing *)CResourceManager::Instance().GetResourcePointer(c_rstrLODModelFileName.c_str());
		}
	}
}

#define ENABLE_LOAD_ALTER_ITEMICON
void CItemData::__SetIconImage(const char * c_szFileName)
{
	if (!CResourceManager::Instance().IsFileExist(c_szFileName))
	{
		TraceError("%s not found. CItemData::__SetIconImage",c_szFileName);
		m_pIconImage = nullptr;
#ifdef ENABLE_LOAD_ALTER_ITEMICON
		static auto c_szAlterIconImage = "icon/item/27995.tga";
		if (CResourceManager::Instance().IsFileExist(c_szAlterIconImage))
			m_pIconImage = (CGraphicSubImage *)CResourceManager::Instance().GetResourcePointer(c_szAlterIconImage);
#endif
	}
	else if (m_pIconImage == nullptr)
		m_pIconImage = (CGraphicSubImage *)CResourceManager::Instance().GetResourcePointer(c_szFileName);
}

void CItemData::SetItemTableData(TItemTable * pItemTable)
{
	memcpy(&m_ItemTable, pItemTable, sizeof(TItemTable));
}

const CItemData::TItemTable* CItemData::GetTable() const
{
	return &m_ItemTable;
}

DWORD CItemData::GetIndex() const
{
	return m_ItemTable.dwVnum;
}

const char * CItemData::GetName() const
{
	// F1 (locale redesign): el cache del servidor manda; el pack es el
	// fallback (un item sin entrada en el bundle se muestra como antes).
	if (ms_pfnLocaleName)
	{
		const char * szLocale = ms_pfnLocaleName(m_ItemTable.dwVnum);
		if (szLocale && szLocale[0] != '\0')
			return szLocale;
	}

	return m_ItemTable.szLocaleName;
}

const char * CItemData::GetDescription() const
{
	return m_strDescription.c_str();
}

const char * CItemData::GetSummary() const
{
	return m_strSummary.c_str();
}

BYTE CItemData::GetType() const
{
	return m_ItemTable.bType;
}

BYTE CItemData::GetSubType() const
{
	return m_ItemTable.bSubType;
}

#define DEF_STR(x) #x

const char* CItemData::GetUseTypeString() const
{
	if (GetType() != CItemData::ITEM_TYPE_USE)
		return "NOT_USE_TYPE";

	switch (GetSubType())
	{
		case USE_TUNING:
			return DEF_STR(USE_TUNING);
		case USE_DETACHMENT:
			return DEF_STR(USE_DETACHMENT);
		case USE_CLEAN_SOCKET:
			return DEF_STR(USE_CLEAN_SOCKET);
		case USE_CHANGE_ATTRIBUTE:
			return DEF_STR(USE_CHANGE_ATTRIBUTE);
		case USE_ADD_ATTRIBUTE:
			return DEF_STR(USE_ADD_ATTRIBUTE);
		case USE_ADD_ATTRIBUTE2:
			return DEF_STR(USE_ADD_ATTRIBUTE2);
		case USE_ADD_ACCESSORY_SOCKET:
			return DEF_STR(USE_ADD_ACCESSORY_SOCKET);
		case USE_PUT_INTO_ACCESSORY_SOCKET:
			return DEF_STR(USE_PUT_INTO_ACCESSORY_SOCKET);
		case USE_PUT_INTO_BELT_SOCKET:
			return DEF_STR(USE_PUT_INTO_BELT_SOCKET);
		case USE_PUT_INTO_RING_SOCKET:
			return DEF_STR(USE_PUT_INTO_RING_SOCKET);
#ifdef ENABLE_USE_COSTUME_ATTR
		case USE_CHANGE_COSTUME_ATTR:
			return DEF_STR(USE_CHANGE_COSTUME_ATTR);
		case USE_RESET_COSTUME_ATTR:
			return DEF_STR(USE_RESET_COSTUME_ATTR);
#endif
	}
	return "USE_UNKNOWN_TYPE";
}

DWORD CItemData::GetWeaponType() const
{
#ifdef ENABLE_WEAPON_COSTUME_SYSTEM
	if (GetType()==CItemData::ITEM_TYPE_COSTUME && GetSubType()==CItemData::COSTUME_WEAPON)
		return GetValue(3);
#endif
	return m_ItemTable.bSubType;
}

BYTE CItemData::GetSize() const
{
	return m_ItemTable.bSize;
}

BOOL CItemData::IsAntiFlag(DWORD dwFlag) const
{
	return (dwFlag & m_ItemTable.dwAntiFlags) != 0;
}

BOOL CItemData::IsFlag(DWORD dwFlag) const
{
	return (dwFlag & m_ItemTable.dwFlags) != 0;
}

BOOL CItemData::IsWearableFlag(DWORD dwFlag) const
{
	return (dwFlag & m_ItemTable.dwWearFlags) != 0;
}

BOOL CItemData::HasNextGrade() const
{
	return 0 != m_ItemTable.dwRefinedVnum;
}

DWORD CItemData::GetWearFlags() const
{
	return m_ItemTable.dwWearFlags;
}

DWORD CItemData::GetIBuyItemPrice() const
{
	return m_ItemTable.dwIBuyItemPrice;
}

DWORD CItemData::GetISellItemPrice() const
{
	return m_ItemTable.dwISellItemPrice;
}

BOOL CItemData::GetLimit(BYTE byIndex, TItemLimit * pItemLimit) const
{
	if (byIndex >= ITEM_LIMIT_MAX_NUM)
	{
		assert(byIndex < ITEM_LIMIT_MAX_NUM);
		return FALSE;
	}

	*pItemLimit = m_ItemTable.aLimits[byIndex];

	return TRUE;
}

BOOL CItemData::GetApply(BYTE byIndex, TItemApply * pItemApply) const
{
	if (byIndex >= ITEM_APPLY_MAX_NUM)
	{
		assert(byIndex < ITEM_APPLY_MAX_NUM);
		return FALSE;
	}

	*pItemApply = m_ItemTable.aApplies[byIndex];
	return TRUE;
}

long CItemData::GetValue(BYTE byIndex) const
{
	if (byIndex >= ITEM_VALUES_MAX_NUM)
	{
		assert(byIndex < ITEM_VALUES_MAX_NUM);
		return 0;
	}

	return m_ItemTable.alValues[byIndex];
}

long CItemData::SetSocket(BYTE byIndex,DWORD value)
{
	if (byIndex >= ITEM_SOCKET_MAX_NUM)
	{
		assert(byIndex < ITEM_SOCKET_MAX_NUM);
		return -1;
	}

	return m_ItemTable.alSockets[byIndex] = value;
}

long CItemData::GetSocket(BYTE byIndex) const
{
	if (byIndex >= ITEM_SOCKET_MAX_NUM)
	{
		assert(byIndex < ITEM_SOCKET_MAX_NUM);
		return -1;
	}

	return m_ItemTable.alSockets[byIndex];
}

int CItemData::GetSocketCount() const
{
	return m_ItemTable.bGainSocketPct;
}

DWORD CItemData::GetIconNumber() const
{
	return m_ItemTable.dwVnum;
//!@#
//	return m_ItemTable.dwIconNumber;
}

UINT CItemData::GetSpecularPoweru() const
{
	return m_ItemTable.bSpecular;
}

float CItemData::GetSpecularPowerf() const
{
	const UINT uSpecularPower=GetSpecularPoweru();

	return float(uSpecularPower) / 100.0f;
}

UINT CItemData::GetRefine() const
{
	return GetIndex()%10;
}

#include <cstring> // for std::strlen
#include <cstdlib> // for std::atoi
#include <span> // for std::span
UINT CItemData::GetRealRefine() const
{
	// Find the position of the "+" character in the string
	const char* plusPos = std::strchr(m_ItemTable.szLocaleName, '+');
	if (!plusPos)
		return 0;

	// Extract the substring after the "+" character
	const std::span<const char> numberSpan(plusPos + 1, std::strlen(plusPos + 1));

	// Convert the substring to an integer
	const int number = atoi(numberSpan.data());
	if (number == 0 && numberSpan[0] != '0')
		return 0;

	return number;
}

bool CItemData::IsArrow() const
{
	return GetType() == ITEM_TYPE_WEAPON && GetSubType() == WEAPON_ARROW;
}

bool CItemData::IsHairDye() const
{
	switch (GetIndex())
	{
	case 70202:
	case 70203:
	case 70204:
	case 70205:
	case 70206:
		return true;
	default:
		return false;
	}
}

bool CItemData::IsSkillBook() const
{
	switch (GetIndex())
	{
	case 50304:
	case 50305:
	case 50306:
	case 50311:
	case 50312:
	case 50313:
	case 50314:
	case 50315:
	case 50316:
	case 50600:
	case 71100:
		return true;
	default:
		return false;
	}
}

bool CItemData::IsClam() const
{
	switch (GetIndex())
	{
	case 27987:
		return true;
	default:
		return false;
	}
}

bool CItemData::IsCorDraconis() const
{
	switch (GetIndex())
	{
	case 30650:	// Cor Draconis Design
	case 50252:	// Cor Draconis
	case 50255:	// Cor Draconis (Rough)
	case 50256:	// Cor Draconis (Cut)
	case 50257:	// Cor Draconis (Rare)
	case 50258:	// Cor Draconis (Antique)
	case 50259:	// Cor Draconis (Legendary)
	case 50260:	// Cor Draconis (Rough)
	case 51501:	// Cor Draconis (Rough)
	case 51502:	// Cor Draconis (Rough)
	case 51503:	// Cor Draconis (Normal)
	case 51504:	// Cor Draconis (Noble)
	case 51505:	// Cor Draconis (Precious)
	case 51506:	// Cor Draconis (Mystical)
	case 51507:	// Cor Draconis (Cut)
	case 51508:	// Cor Draconis (Rare)
	case 51509:	// Cor Draconis (Antique)
	case 51510:	// Cor Draconis (Legendary)
	case 51541:	// Cor Draconis (Glowing)
	case 51548:	// Cor Draconis (Flawless)
	case 51549:	// Cor Draconis (Eternal)
	case 51562:	// Cor Draconis (Epic)
	case 51569:	// Cor Draconis (Divine)
	case 51576:	// Cor Draconis+ (Normal)
	case 51583:	// Cor Draconis+ (Noble)
	case 51590:	// Cor Draconis+ (Precious)
	case 51597:	// Cor Draconis+ (Flawless)
	case 51604:	// Cor Draconis+ (Glowing)
	case 51611:	// Cor Draconis+ (Eternal)
	case 51618:	// Cor Draconis+ (Mystical)
	case 51625:	// Cor Draconis+ (Epic)
	case 51632:	// Cor Draconis+ (Divine)
	case 76040:	// Cor Draconis (Normal)
	case 83014:	// Cor Draconis Chest
		return true;
	default:
		return false;
	}
}

bool CItemData::IsMarriage() const
{
	switch (GetIndex())
	{
	case 11901:	// Tuxedo
	case 11902:	// Tuxedo
	case 11903:	// Wedding Dress
	case 11904:	// Wedding Dress
	case 11911:	// Tuxedo
	case 11912:	// Tuxedo
	case 11913:	// Wedding Dress
	case 11914:	// Wedding Dress
	case 30054:	// Wedding Ring
	case 50201:	// Bouquet
	case 50202:	// Bouquet
	case 70301:	// Engagement Ring
	case 70302:	// Wedding Ring
	case 71068:	// Feather of Lovers
	case 71069:	// Earrings of Harmony
	case 71070:	// Love Bracelet
	case 71071:	// Earrings of Love
	case 71072:	// Harmony Bracelet
	case 71073:	// Necklace of Love
	case 71074:	// Necklace of Harmony
	case 71145:	// Amulet of Eternal Love
	case 72010:	// Lovebird Feather
	case 72011:	// Lovebird Feather
	case 72012:	// Lovebird Feather
		return true;
	default:
		return false;
	}
}

bool CItemData::IsParty() const
{
	return false;
}

bool CItemData::IsCraft() const
{
	return false;
}

bool CItemData::IsScroll() const
{
	return false;
}

UINT CItemData::GetLevelLimit() const
{
	for (auto & limit : m_ItemTable.aLimits)
	{
		if (limit.bType == LIMIT_LEVEL)
			return limit.lValue;
	}
	return 0;
}

BOOL CItemData::IsEquipment() const
{
	switch (GetType())
	{
		case ITEM_TYPE_WEAPON:
		case ITEM_TYPE_ARMOR:
			return TRUE;
			break;
	}

	return FALSE;
}

#ifdef ENABLE_ACCE_COSTUME_SYSTEM
float CItemData::GetItemParticleScale(BYTE byJob, BYTE bySex) const
{
	return m_ItemScaleTable.fScaleParticle[bySex][byJob];
}

void CItemData::SetItemTableScaleData(BYTE bJob, BYTE bSex, float fScaleX, float fScaleY, float fScaleZ, float fScaleParticle)
{
	m_ItemScaleTable.v3Scale[bSex][bJob].x = fScaleX;
	m_ItemScaleTable.v3Scale[bSex][bJob].y = fScaleY;
	m_ItemScaleTable.v3Scale[bSex][bJob].z = fScaleZ;
	m_ItemScaleTable.fScaleParticle[bSex][bJob] = fScaleParticle;
}

D3DXVECTOR3& CItemData::GetItemScaleVector(BYTE bJob, BYTE bSex)
{
	return m_ItemScaleTable.v3Scale[bSex][bJob];
}
#endif

void CItemData::Clear()
{
	m_strSummary = "";
	m_strModelFileName = "";
	m_strSubModelFileName = "";
	m_strDropModelFileName = "";
	m_strIconFileName = "";
	m_strLODModelFileNameVector.clear();

	m_pModelThing = nullptr;
	m_pSubModelThing = nullptr;
	m_pDropModelThing = nullptr;
	m_pIconImage = nullptr;
	m_pLODModelThingVector.clear();

	memset(&m_ItemTable, 0, sizeof(m_ItemTable));
#ifdef ENABLE_ACCE_COSTUME_SYSTEM
	memset(&m_ItemScaleTable, 0, sizeof(m_ItemScaleTable));
#endif
}

CItemData::CItemData()
{
	Clear();
}

CItemData::~CItemData()
{
}

