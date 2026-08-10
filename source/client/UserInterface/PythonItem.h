#pragma once

#include "../EterGrnLib/ThingInstance.h"

#if defined(__BL_OFFICIAL_LOOT_FILTER__)
#include "rapidjson/document.h"
#endif

class CItemData;

class CPythonItem : public CSingleton<CPythonItem>
{
	public:
		enum
		{
			INVALID_ID = 0xffffffff,
		};

		enum
		{
			VNUM_MONEY = 1,
		};

		enum
		{
			USESOUND_NONE,
			USESOUND_DEFAULT,
			USESOUND_ARMOR,
			USESOUND_WEAPON,
			USESOUND_BOW,
			USESOUND_ACCESSORY,
			USESOUND_POTION,
			USESOUND_PORTAL,
			USESOUND_NUM,
		};

		enum
		{
			DROPSOUND_DEFAULT,
			DROPSOUND_ARMOR,
			DROPSOUND_WEAPON,
			DROPSOUND_BOW,
			DROPSOUND_ACCESSORY,
			DROPSOUND_NUM
		};

		typedef struct SGroundItemInstance
		{
			DWORD					dwVirtualNumber;
			D3DXVECTOR3				v3EndPosition;

			D3DXVECTOR3				v3RotationAxis;
			D3DXQUATERNION			qEnd;
			D3DXVECTOR3				v3Center;
			CGraphicThingInstance	ThingInstance;
			DWORD					dwStartTime;
			DWORD					dwEndTime;

			DWORD					eDropSoundType;

			bool					bAnimEnded;
			bool Update();
			void Clear();

			DWORD					dwEffectInstanceIndex;
			std::string				stOwnership;

			#ifdef ENABLE_ITEM_GROUND_EX
			DWORD count{};
			std::array<long, ITEM_SOCKET_SLOT_MAX_NUM> sockets{};
			std::array<TPlayerItemAttribute, ITEM_ATTRIBUTE_SLOT_MAX_NUM> attrs{};
			#endif

			static void	__PlayDropSound(DWORD eItemType, const D3DXVECTOR3& c_rv3Pos);
			static std::string		ms_astDropSoundFileName[DROPSOUND_NUM];
		} TGroundItemInstance;

		typedef std::map<DWORD, TGroundItemInstance *>	TGroundItemInstanceMap;

	public:
		CPythonItem(void);
		virtual ~CPythonItem(void);

		// Initialize
		void	Destroy();
		void	Create();

		void	PlayUseSound(DWORD dwItemID) const;
		void	PlayDropSound(DWORD dwItemID) const;
		void	PlayUsePotionSound() const;

		void	SetUseSoundFileName(DWORD eItemType, const std::string& c_rstFileName);
		void	SetDropSoundFileName(DWORD eItemType, const std::string& c_rstFileName) const;

		void	GetInfo(std::string* pstInfo);

		void	DeleteAllItems();

		void	Render();
		void	Update(const POINT& c_rkPtMouse);

		void	CreateItem(DWORD dwVirtualID, DWORD dwVirtualNumber, float x, float y, float z, bool bDrop=true
			#ifdef ENABLE_ITEM_GROUND_EX
			, DWORD count = 1
			, std::array<TPlayerItemAttribute, ITEM_ATTRIBUTE_SLOT_MAX_NUM> aAttrs = {}
			, std::array<long, ITEM_SOCKET_SLOT_MAX_NUM> alSockets = {}
			#endif
		);
		void	DeleteItem(DWORD dwVirtualID);
		void	SetOwnership(DWORD dwVID, const char * c_pszName);
		bool	GetOwnership(DWORD dwVID, const char ** c_pszName);

		BOOL	GetGroundItemPosition(DWORD dwVirtualID, TPixelPosition * pPosition);

		bool	GetPickedItemID(DWORD* pdwPickedItemID) const;

		bool	GetCloseItem(const TPixelPosition & c_rPixelPosition, DWORD* pdwItemID, DWORD dwDistance=300);
		bool	GetCloseMoney(const TPixelPosition & c_rPixelPosition, DWORD* dwItemID, DWORD dwDistance=300);

		DWORD	GetVirtualNumberOfGroundItem(DWORD dwVID);

		void	BuildNoGradeNameData(int iType) const;
		DWORD	GetNoGradeNameDataCount() const;
		CItemData * GetNoGradeNameDataPtr(DWORD dwIndex) const;

	protected:
		DWORD	__Pick(const POINT& c_rkPtMouse);

		DWORD	__GetUseSoundType(const CItemData& c_rkItemData) const;
		DWORD	__GetDropSoundType(const CItemData& c_rkItemData) const;

	protected:
		TGroundItemInstanceMap				m_GroundItemInstanceMap;
		CDynamicPool<TGroundItemInstance>	m_GroundItemInstancePool;

		DWORD m_dwDropItemEffectID;
		DWORD m_dwPickedItemID;

		int m_nMouseX;
		int m_nMouseY;

		std::string m_astUseSoundFileName[USESOUND_NUM];

		std::vector<CItemData *> m_NoGradeNameItemData;

#ifdef ENABLE_MULTI_ITEM_PICK
	public:
		std::vector<uint32_t> vecMultiItemPick;
#endif

#if defined(__BL_OFFICIAL_LOOT_FILTER__)
	public:
		void SaveLootingSettings(const std::string_view jsonData);
		const rapidjson::Document& GetLootingSettings() const { return mLootingData; }
		bool CanLoot(const TGroundItemInstanceMap::iterator& item) const;
	protected:
		rapidjson::Document mLootingData;
#endif
};

