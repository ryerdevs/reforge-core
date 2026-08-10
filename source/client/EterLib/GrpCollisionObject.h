#pragma once

#include "GrpBase.h"

class CGraphicCollisionObject : public CGraphicBase
{
	public:
		CGraphicCollisionObject();
		virtual ~CGraphicCollisionObject();

	protected:
		bool IntersectTriangle(const D3DXVECTOR3& c_orig, const D3DXVECTOR3& c_dir, const D3DXVECTOR3& c_v0, const D3DXVECTOR3& c_v1, const D3DXVECTOR3& c_v2, float* pu, float* pv, float* pt) const;
		bool IntersectBoundBox(const D3DXMATRIX* c_pmatWorld, const TBoundBox& c_rboundBox, float* pu, float* pv, float* pt) const;
		bool IntersectCube(const D3DXMATRIX* c_pmatWorld, float sx, float sy, float sz, float ex, float ey, float ez, D3DXVECTOR3 & RayOriginal, D3DXVECTOR3 & RayDirection, float* pu, float* pv, float* pt) const;
		bool IntersectIndexedMesh(const D3DXMATRIX* c_pmatWorld, const void* vertices, int step, int vtxCount, const void* indices, int idxCount, D3DXVECTOR3 & RayOriginal, D3DXVECTOR3 & RayDirection, float* pu, float* pv, float* pt) const;
		bool IntersectMesh(const D3DXMATRIX * c_pmatWorld, const void * vertices, DWORD dwStep, DWORD dwvtxCount, D3DXVECTOR3 & RayOriginal, D3DXVECTOR3 & RayDirection, float* pu, float* pv, float* pt) const;

		bool IntersectSphere(const D3DXVECTOR3 & c_rv3Position, float fRadius, const D3DXVECTOR3 & c_rv3RayOriginal, const D3DXVECTOR3 & c_rv3RayDirection) const;
		bool IntersectCylinder(const D3DXVECTOR3 & c_rv3Position, float fRadius, float fHeight, const D3DXVECTOR3 & c_rv3RayOriginal, const D3DXVECTOR3 & c_rv3RayDirection) const;

		bool IntersectSphere(const D3DXVECTOR3 & c_rv3Position, float fRadius) const;
		bool IntersectCylinder(const D3DXVECTOR3 & c_rv3Position, float fRadius, float fHeight) const;
};

