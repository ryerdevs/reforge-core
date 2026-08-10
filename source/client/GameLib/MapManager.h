#pragma once

class CMapBase;
#include "MapOutdoor.h"
#include "PropertyManager.h"

// VICTIM_COLLISION_TEST
#include "PhysicsObject.h"
// VICTIM_COLLISION_TEST_END

// Map Manager
class CMapManager : public CScreen, public IPhysicsWorld
{
	public:
		CMapManager();
		virtual ~CMapManager();

		bool IsMapOutdoor() const;
		CMapOutdoor& GetMapOutdoorRef() const;

		bool	IsSoftwareTilingEnable() const;
		void	ReserveSoftwareTilingEnable(bool isEnable);

		//////////////////////////////////////////////////////////////////////////
		// Contructor / Destructor
		//////////////////////////////////////////////////////////////////////////
		void					Initialize();
		void					Destroy();

		void					Create();

		virtual void			Clear();
		virtual CMapBase *		AllocMap();

		//////////////////////////////////////////////////////////////////////////
		//////////////////////////////////////////////////////////////////////////
		bool					IsMapReady() const;

		virtual bool			LoadMap(const std::string & c_rstrMapName, float x, float y, float z);
		bool					UnloadMap(const std::string c_strMapName);

		bool					UpdateMap(float fx, float fy, float fz) const;
		void					UpdateAroundAmbience(float fx, float fy, float fz) const;
		float					GetHeight(float fx, float fy) const;
		float					GetTerrainHeight(float fx, float fy) const;
		bool					GetWaterHeight(int iX, int iY, long * plWaterHeight) const;

		bool					GetNormal(int ix, int iy, D3DXVECTOR3 * pv3Normal) const;

		//////////////////////////////////////////////////////////////////////////
		// Environment
		///
		void					SetEnvironmentDataPtr(const TEnvironmentData * c_pEnvironmentData);
		void					ResetEnvironmentDataPtr(const TEnvironmentData * c_pEnvironmentData);
		void					SetEnvironmentData(int nEnvDataIndex);

		void					BeginEnvironment();
		void					EndEnvironment() const;

		void					BlendEnvironmentData(const TEnvironmentData * c_pEnvironmentData, int iTransitionTime) const;

		void					GetCurrentEnvironmentData(const TEnvironmentData ** c_ppEnvironmentData) const;
		bool					RegisterEnvironmentData(DWORD dwIndex, const char * c_szFileName);
		bool					GetEnvironmentData(DWORD dwIndex, const TEnvironmentData ** c_ppEnvironmentData);

		// Portal
		void					RefreshPortal() const;
		void					ClearPortal() const;
		void					AddShowingPortalID(int iID) const;

		// External interface
		void					LoadProperty();

		DWORD					GetShadowMapColor(float fx, float fy) const;

		// VICITM_COLLISION_TEST
		virtual bool isPhysicalCollision(const D3DXVECTOR3 & c_rvCheckPosition);
		// VICITM_COLLISION_TEST_END

		bool					isAttrOn(float fX, float fY, BYTE byAttr) const;
		bool					GetAttr(float fX, float fY, BYTE * pbyAttr) const;
		bool					isAttrOn(int iX, int iY, BYTE byAttr) const;
		bool					GetAttr(int iX, int iY, BYTE * pbyAttr) const;

		std::vector<int> &		GetRenderedSplatNum(int * piPatch, int * piSplat, float * pfSplatRatio) const;
		CArea::TCRCWithNumberVector & GetRenderedGraphicThingInstanceNum(DWORD * pdwGraphicThingInstanceNum, DWORD * pdwCRCNum) const;

	protected:
		TEnvironmentData *		AllocEnvironmentData() const;
		void					DeleteEnvironmentData(TEnvironmentData * pEnvironmentData) const;
		BOOL					LoadEnvironmentData(const char * c_szFileName, TEnvironmentData * pEnvironmentData) const;

	protected:
		CPropertyManager			m_PropertyManager;

		//////////////////////////////////////////////////////////////////////////
		// Environment
		//////////////////////////////////////////////////////////////////////////
		TEnvironmentDataMap			m_EnvironmentDataMap;
		const TEnvironmentData *	mc_pcurEnvironmentData;

		//////////////////////////////////////////////////////////////////////////
		// Map
		//////////////////////////////////////////////////////////////////////////
		CMapOutdoor *				m_pkMap;
		CSpeedTreeDirectX			m_Forest;

	public:
		// 2004.10.14.myevan.TEMP_CAreaLoaderThread
		//bool	BGLoadingEnable();
		//void	BGLoadingEnable(bool bBGLoadingEnable);
		void	SetTerrainRenderSort(CMapOutdoor::ETerrainRenderSort eTerrainRenderSort) const;
		CMapOutdoor::ETerrainRenderSort	GetTerrainRenderSort() const;

		void	GetBaseXY(DWORD * pdwBaseX, DWORD * pdwBaseY) const;

	public:
		void	SetTransparentTree(bool bTransparenTree) const;

	public:
		typedef struct SMapInfo
		{
			std::string	m_strName;
			DWORD		m_dwBaseX;
			DWORD		m_dwBaseY;
			DWORD		m_dwSizeX;
			DWORD		m_dwSizeY;
			DWORD		m_dwEndX;
			DWORD		m_dwEndY;
		} TMapInfo;
		typedef std::vector<TMapInfo>		TMapInfoVector;
		typedef TMapInfoVector::iterator	TMapInfoVectorIterator;

	protected:
		TMapInfoVector			m_kVct_kMapInfo;

		bool m_isSoftwareTilingEnableReserved;

	protected:
		void	__LoadMapInfoVector();

	protected:
		struct FFindMapName
		{
			std::string strNametoFind;
			FFindMapName(const std::string & c_rMapName)
			{
				strNametoFind = c_rMapName;
				stl_lowers(strNametoFind);
			}
			bool operator() (TMapInfo & rMapInfo) const
			{
				if (rMapInfo.m_strName == strNametoFind)
					return true;
				return false;
			}
		};
	public:
		void SetAtlasInfoFileName(const char* filename)
		{
			m_stAtlasInfoFileName = filename;
		}
	private:
		std::string m_stAtlasInfoFileName;
};

