#pragma once

#include "GrpCollisionObject.h"
#include "../SphereLib/frustum.h"

class CScreen : public CGraphicCollisionObject
{
public:
	CScreen();
	virtual ~CScreen();

	void ClearDepthBuffer() const;
	void Clear() const;
	bool Begin() const;
	void End() const;
	void Show(HWND hWnd = nullptr) const;
	void Show(RECT * pSrcRect) const;
	void Show(RECT * pSrcRect, HWND hWnd) const;

	void RenderLine2d(float sx, float sy, float ex, float ey, float z=0.0f) const;
	void RenderBox2d(float sx, float sy, float ex, float ey, float z=0.0f) const;
	void RenderBar2d(float sx, float sy, float ex, float ey, float z=0.0f) const;
	void RenderGradationBar2d(float sx, float sy, float ex, float ey, DWORD dwStartColor, DWORD dwEndColor, float ez=0.0f) const;
	void RenderCircle2d(float fx, float fy, float fz, float fRadius, int iStep = 50) const;
	void RenderCircle3d(float fx, float fy, float fz, float fRadius, int iStep = 50) const;

	void RenderLine3d(float sx, float sy, float sz, float ex, float ey, float ez) const;
	void RenderBox3d(float sx, float sy, float sz, float ex, float ey, float ez) const;
	void RenderBar3d(float sx, float sy, float sz, float ex, float ey, float ez) const;
	void RenderBar3d(const D3DXVECTOR3 * c_pv3Positions) const;
	void RenderGradationBar3d(float sx, float sy, float sz, float ex, float ey, float ez, DWORD dwStartColor, DWORD dwEndColor) const;

	void RenderLineCube(float sx, float sy, float sz, float ex, float ey, float ez) const;
	void RenderCube(float sx, float sy, float sz, float ex, float ey, float ez) const;
	void RenderCube(float sx, float sy, float sz, float ex, float ey, float ez, D3DXMATRIX matRotation) const;
	void RenderTextureBox(float sx, float sy, float ex, float ey, float z=0.0f, float su=0.0f, float sv=0.0f, float eu=1.0f, float ev=1.0f) const;
	void RenderBillboard(D3DXVECTOR3 * Position, D3DXCOLOR & Color) const;

	void DrawMinorGrid(float xMin, float yMin, float xMax, float yMax, float xminorStep, float yminorStep, float zPos=0) const;
	void DrawGrid(float xMin, float yMin, float xMax, float yMax, float xmajorStep, float ymajorStep, float xminorStep, float yminorStep, float zPos=0) const;

	void RenderD3DXMesh(LPD3DXMESH lpMesh, const D3DXMATRIX * c_pmatWorld, float fx, float fy, float fz, float fRadius, D3DFILLMODE d3dFillMode) const;
	void RenderSphere(const D3DXMATRIX * c_pmatWorld, float fx, float fy, float fz, float fRadius, D3DFILLMODE d3dFillMode = D3DFILL_SOLID) const;
	void RenderCylinder(const D3DXMATRIX * c_pmatWorld, float fx, float fy, float fz, float fRadius, float fLength, D3DFILLMODE d3dFillMode = D3DFILL_SOLID) const;

	void SetColorOperation() const;
	void SetDiffuseOperation() const;
	void SetBlendOperation() const;
	void SetOneColorOperation(D3DXCOLOR & rColor) const;
	void SetAddColorOperation(D3DXCOLOR & rColor) const;
	void SetDiffuseColor(DWORD diffuseColor) const;
	void SetDiffuseColor(float r, float g, float b, float a=1.0f) const;
	void SetClearColor(float r, float g, float b, float a=1.0f) const;
	void SetClearDepth(float depth) const;
	void SetClearStencil(DWORD stencil) const;

	void SetCursorPosition(int x, int y, int hres, int vres) const;	// creates picking ray
	bool GetCursorPosition(float* px, float* py, float* pz) const;
	bool GetCursorXYPosition(float* px, float* py) const;
	bool GetCursorZPosition(float* pz) const;
	void GetPickingPosition(float t, float* x, float* y, float* z) const;
	void ProjectPosition(float x, float y, float z, float * pfX, float * pfY) const;
	void ProjectPosition(float x, float y, float z, float * pfX, float * pfY, float * pfZ) const;
	void UnprojectPosition(float x, float y, float z, float * pfX, float * pfY, float * pfZ) const;

	BOOL IsLostDevice() const;
	BOOL RestoreDevice() const;

	void BuildViewFrustum() const;

	static void Identity();
	static Frustum & GetFrustum() { return ms_frustum; }

protected:
	static DWORD		ms_diffuseColor;
	static DWORD		ms_clearColor;
	static DWORD		ms_clearStencil;
	static float		ms_clearDepth;

	static Frustum ms_frustum;
};

