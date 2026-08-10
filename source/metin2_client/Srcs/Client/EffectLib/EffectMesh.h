#pragma once

#include <d3dx9.h>

#include "../eterlib/GrpScreen.h"
#include "../eterlib/Resource.h"
#include "../eterlib/GrpImageInstance.h"
#include "../eterLib/TextFileLoader.h"

#include "Type.h"
#include "EffectElementBase.h"

class CEffectMesh : public CResource
{
	public:
		typedef struct SEffectFrameData
		{
			BYTE byChangedFrame;
			float fVisibility;
			DWORD dwVertexCount;
			DWORD dwTextureVertexCount;
			DWORD dwIndexCount;
			std::vector<TPTVertex> PDTVertexVector;
		} TEffectFrameData;

		typedef struct SEffectMeshData
		{
			char szObjectName[32];
			char szDiffuseMapFileName[128];

			std::vector<TEffectFrameData> EffectFrameDataVector;
			std::vector<CGraphicImage*> pImageVector;

			static SEffectMeshData* New();
			static void Delete(SEffectMeshData* pkData);

			static void DestroySystem();

			static CDynamicPool<SEffectMeshData> ms_kPool;
		} TEffectMeshData;

	// About Resource Code
	public:
		typedef CRef<CEffectMesh> TRef;

	public:
		static TType Type();

	public:
		CEffectMesh(const char * c_szFileName);
		virtual ~CEffectMesh();

		DWORD GetFrameCount() const;
		DWORD GetMeshCount() const;
		TEffectMeshData * GetMeshDataPointer(DWORD dwMeshIndex) const;

		std::vector<CGraphicImage*>* GetTextureVectorPointer(DWORD dwMeshIndex) const;
		std::vector<CGraphicImage*>& GetTextureVectorReference(DWORD dwMeshIndex) const;

		// Exceptional function for tool
		BOOL GetMeshElementPointer(DWORD dwMeshIndex, TEffectMeshData ** ppMeshData) const;

	protected:
		bool OnLoad(int iSize, const void * c_pvBuf);

		void OnClear();
		bool OnIsEmpty() const;
		bool OnIsType(TType type);

		BOOL __LoadData_Ver001(int iSize, const BYTE * c_pbBuf);
		BOOL __LoadData_Ver002(int iSize, const BYTE * c_pbBuf);

	protected:
		int								m_iGeomCount;
		int								m_iFrameCount;
		std::vector<TEffectMeshData *>	m_pEffectMeshDataVector;

		bool							m_isData;
};

class CEffectMeshScript : public CEffectElementBase
{
	public:
		typedef struct SMeshData
		{
			BYTE byBillboardType;

			BOOL bBlendingEnable;
			BYTE byBlendingSrcType;
			BYTE byBlendingDestType;
			BOOL bTextureAlphaEnable;

			BYTE byColorOperationType;
			D3DXCOLOR ColorFactor;

			BOOL bTextureAnimationLoopEnable;
			float fTextureAnimationFrameDelay;

			DWORD dwTextureAnimationStartFrame;

			TTimeEventTableFloat TimeEventAlpha;

			SMeshData()
			{
				TimeEventAlpha.clear();
			}
		} TMeshData;
		typedef std::vector<TMeshData> TMeshDataVector;

	public:
		CEffectMeshScript();
		virtual ~CEffectMeshScript();

		const char * GetMeshFileName() const;

		void ReserveMeshData(DWORD dwMeshCount);
		bool CheckMeshIndex(DWORD dwMeshIndex) const;
		bool GetMeshDataPointer(DWORD dwMeshIndex, TMeshData ** ppMeshData);
		int GetMeshDataCount() const;

		int GetBillboardType(DWORD dwMeshIndex) const;
		BOOL isBlendingEnable(DWORD dwMeshIndex) const;
		BYTE GetBlendingSrcType(DWORD dwMeshIndex) const;
		BYTE GetBlendingDestType(DWORD dwMeshIndex) const;
		BOOL isTextureAlphaEnable(DWORD dwMeshIndex) const;
		BOOL GetColorOperationType(DWORD dwMeshIndex, BYTE * pbyType) const;
		BOOL GetColorFactor(DWORD dwMeshIndex, D3DXCOLOR * pColor) const;
		BOOL GetTimeTableAlphaPointer(DWORD dwMeshIndex, TTimeEventTableFloat ** pTimeEventAlpha);

		BOOL isMeshAnimationLoop() const;
		BOOL GetMeshAnimationLoopCount() const;
		float GetMeshAnimationFrameDelay() const;
		BOOL isTextureAnimationLoop(DWORD dwMeshIndex) const;
		float GetTextureAnimationFrameDelay(DWORD dwMeshIndex) const;
		DWORD GetTextureAnimationStartFrame(DWORD dwMeshIndex) const;

	protected:
		void OnClear();
		bool OnIsData();
		BOOL OnLoadScript(CTextFileLoader & rTextFileLoader);

	protected:
		BOOL m_isMeshAnimationLoop;
		int m_iMeshAnimationLoopCount;
		float m_fMeshAnimationFrameDelay;
		TMeshDataVector m_MeshDataVector;

		std::string m_strMeshFileName;

	public:
		static void DestroySystem();

		static CEffectMeshScript* New();
		static void Delete(CEffectMeshScript* pkData);

		static CDynamicPool<CEffectMeshScript> ms_kPool;
};

