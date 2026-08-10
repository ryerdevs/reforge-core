#pragma once

//#define CACHE_DEFORMED_VERTEX
#include "../eterlib/GrpImage.h"
#include "../eterlib/GrpCollisionObject.h"

#include "Model.h"
#include "Motion.h"

class CGrannyModelInstance : public CGraphicCollisionObject
{
	public:
		enum
		{
			ANIFPS_MIN = 30,
			ANIFPS_MAX = 120,
		};
	public:
		static void DestroySystem();

		static CGrannyModelInstance* New();
		static void Delete(CGrannyModelInstance* pkInst);

		static CDynamicPool<CGrannyModelInstance>		ms_kPool;

	public:
		struct FCreateDeviceObjects
		{
			void operator() (CGrannyModelInstance * pModelInstance) const
			{pModelInstance->CreateDeviceObjects();}
		};

		struct FDestroyDeviceObjects
		{
			void operator() (CGrannyModelInstance * pModelInstance) const
			{pModelInstance->DestroyDeviceObjects();}
		};

	public:
		CGrannyModelInstance();
		virtual ~CGrannyModelInstance();

		bool	IsEmpty() const;
		void	Clear();

		bool	CreateDeviceObjects();
		void	DestroyDeviceObjects();

		// Update & Render
		void	Update(DWORD dwAniFPS);
		void	UpdateLocalTime(float fElapsedTime);
		void	UpdateTransform(D3DXMATRIX * pMatrix, float fSecondsElapsed) const;

		void	UpdateSkeleton(const D3DXMATRIX * c_pWorldMatrix, float fLocalTime) const;
		void	DeformNoSkin(const D3DXMATRIX * c_pWorldMatrix);
		void	Deform(const D3DXMATRIX * c_pWorldMatrix);

		void	RenderWithOneTexture();
		void	RenderWithTwoTexture();
		void	BlendRenderWithOneTexture();
		void	BlendRenderWithTwoTexture();
		void	RenderWithoutTexture();

		// Model
		CGrannyModel* GetModel() const;
		void	SetMaterialImagePointer(const char* c_szImageName, CGraphicImage* pImage);
		void	SetMaterialData(const char* c_szImageName, const SMaterialData& c_rkMaterialData);
		void	SetSpecularInfo(const char* c_szMtrlName, BOOL bEnable, float fPower) const;

		void	SetMainModelPointer(CGrannyModel* pkModel, CGraphicVertexBuffer* pkSharedDefromableVertexBuffer);
		void	SetLinkedModelPointer(CGrannyModel* pkModel, CGraphicVertexBuffer* pkSharedDefromableVertexBuffer, CGrannyModelInstance** ppkSkeletonInst);

		// Motion
		void	SetMotionPointer(const CGrannyMotion* pMotion, float blendTime=0.0f, int loopCount=0, float speedRatio=1.0f);
		void	ChangeMotionPointer(const CGrannyMotion* pMotion, int loopCount=0, float speedRatio=1.0f);
		void	SetMotionAtEnd() const;
		bool	IsMotionPlaying() const;

		void	CopyMotion(CGrannyModelInstance * pModelInstance, bool bIsFreeSourceControl=false);

		// Time
		void	SetLocalTime(float fLocalTime);
		int		ResetLocalTime();
		float	GetLocalTime() const;
		float	GetNextTime();

		// WORK
		DWORD	GetDeformableVertexCount() const;
		DWORD	GetVertexCount() const;

		// END_OF_WORK

		// Bone & Attaching
		const float *	GetBoneMatrixPointer(int iBone) const;
		const float *	GetCompositeBoneMatrixPointer(int iBone) const;
		bool			GetMeshMatrixPointer(int iMesh, const D3DXMATRIX ** c_ppMatrix) const;
		bool			GetBoneIndexByName(const char * c_szBoneName, int * pBoneIndex) const;
		void			SetParentModelInstance(const CGrannyModelInstance* c_pParentModelInstance, const char * c_szBoneName);
		void			SetParentModelInstance(const CGrannyModelInstance* c_pParentModelInstance, int iBone);

		// Collision Detection
		bool	Intersect(const D3DXMATRIX * c_pMatrix, float * pu, float * pv, float * pt);
		void	MakeBoundBox(TBoundBox* pBoundBox, const float* mat, const float* OBBMin, const float* OBBMax, D3DXVECTOR3* vtMin, D3DXVECTOR3* vtMax) const;
		void	GetBoundBox(D3DXVECTOR3 * vtMin, D3DXVECTOR3* vtMax);

		// Reload Texture
		void	ReloadTexture() const;

	protected:
		void	__Initialize();

		void	__DestroyModelInstance();
		void	__DestroyMeshMatrices();
		void	__DestroyDynamicVertexBuffer();

		void	__CreateModelInstance();
		void	__CreateMeshMatrices();
		void	__CreateDynamicVertexBuffer();

		// WORK
		void	__DestroyWorldPose();
		void	__CreateWorldPose(CGrannyModelInstance* pkSrcModelInst);

		bool	__CreateMeshBindingVector(CGrannyModelInstance* pkDstModelInst);
		void	__DestroyMeshBindingVector();

#if GrannyProductMinorVersion==4
		int*	__GetMeshBoneIndices(unsigned int iMeshBinding) const;
#elif GrannyProductMinorVersion==11 || GrannyProductMinorVersion==9 || GrannyProductMinorVersion==8 || GrannyProductMinorVersion==7
		const granny_int32x*	__GetMeshBoneIndices(unsigned int iMeshBinding) const;
#else
#error "unknown granny version"
#endif

		bool	__IsDeformableVertexBuffer() const;
		void	__SetSharedDeformableVertexBuffer(CGraphicVertexBuffer* pkSharedDeformableVertexBuffer);

		IDirect3DVertexBuffer9* __GetDeformableD3DVertexBufferPtr();
		CGraphicVertexBuffer&	__GetDeformableVertexBufferRef();

		granny_world_pose* __GetWorldPosePtr() const;
		// END_OF_WORK

		// Update & Render
		void	UpdateWorldPose() const;
		void	UpdateWorldMatrices(const D3DXMATRIX * c_pWorldMatrix) const;
		void	DeformPNTVertices(void * pvDest) const;

		void	RenderMeshNodeListWithOneTexture(CGrannyMesh::EType eMeshType, CGrannyMaterial::EType eMtrlType) const;
		void	RenderMeshNodeListWithTwoTexture(CGrannyMesh::EType eMeshType, CGrannyMaterial::EType eMtrlType) const;
		void	RenderMeshNodeListWithoutTexture(CGrannyMesh::EType eMeshType, CGrannyMaterial::EType eMtrlType) const;

	protected:
		// Static Data
		CGrannyModel *					m_pModel;

		// Granny Data
		granny_model_instance *			m_pgrnModelInstance;

		granny_control *				m_pgrnCtrl;
		granny_animation *				m_pgrnAni;

		// Meshes' Transform Data
		D3DXMATRIX *					m_meshMatrices;

		// Attaching Data
		const CGrannyModelInstance *	mc_pParentInstance;
		int								m_iParentBoneIndex;

		// Game Data
		float							m_fLocalTime;
		float							m_fSecondsElapsed;

		DWORD							m_dwOldUpdateFrame;

		CGrannyMaterialPalette			m_kMtrlPal;

		// WORK
		granny_world_pose*					m_pgrnWorldPoseReal;
		std::vector<granny_mesh_binding*>	m_vct_pgrnMeshBinding;

		// Dynamic Vertex Buffer
		CGraphicVertexBuffer*			m_pkSharedDeformableVertexBuffer;
		CGraphicVertexBuffer			m_kLocalDeformableVertexBuffer;
		bool							m_isDeformableVertexBuffer;
		// END_OF_WORK

		SMaterialData m_material_data{}; // @fixme057

		// TEST
		CGrannyModelInstance** m_ppkSkeletonInst;
		// END_OF_TEST
#ifdef _TEST
		D3DXMATRIX TEST_matWorld;
#endif
	public:
		bool							HaveBlendThing() const { return m_pModel->HaveBlendThing(); }
};

