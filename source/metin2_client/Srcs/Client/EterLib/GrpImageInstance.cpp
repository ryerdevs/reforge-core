#include "StdAfx.h"
#include "GrpImageInstance.h"
#include "StateManager.h"

#include "../eterBase/CRC32.h"
//STATEMANAGER.SaveRenderState(D3DRS_SRCBLEND, D3DBLEND_INVDESTCOLOR);
//STATEMANAGER.SaveRenderState(D3DRS_DESTBLEND, D3DBLEND_ONE);
//STATEMANAGER.RestoreRenderState(D3DRS_SRCBLEND);
//STATEMANAGER.RestoreRenderState(D3DRS_DESTBLEND);

CDynamicPool<CGraphicImageInstance>		CGraphicImageInstance::ms_kPool;

void CGraphicImageInstance::CreateSystem(UINT uCapacity)
{
	ms_kPool.Create(uCapacity);
}

void CGraphicImageInstance::DestroySystem()
{
	ms_kPool.Destroy();
}

CGraphicImageInstance* CGraphicImageInstance::New()
{
	return ms_kPool.Alloc();
}

void CGraphicImageInstance::Delete(CGraphicImageInstance* pkImgInst)
{
	pkImgInst->Destroy();
	ms_kPool.Free(pkImgInst);
}

#if defined(__BL_CLIP_MASK__)
void CGraphicImageInstance::Render(RECT* rMask)
#else
void CGraphicImageInstance::Render()
#endif
{
	if (IsEmpty())
		return;

	assert(!IsEmpty());

#if defined(__BL_CLIP_MASK__)
	OnRender(rMask);
#else
	OnRender();
#endif
}

#if defined(__BL_CLIP_MASK__)
void CGraphicImageInstance::OnRender(RECT* rMask)
#else
void CGraphicImageInstance::OnRender()
#endif
{
	CGraphicImage * pImage = m_roImage.GetPointer();
	const CGraphicTexture * pTexture = pImage->GetTexturePointer();

#ifdef ENABLE_UI_MOVING
	const float fimgWidth = pImage->GetWidth() * m_v2Scale.x;
	const float fimgHeight = pImage->GetHeight() * m_v2Scale.y;
#else
	const float fimgWidth = pImage->GetWidth();
	const float fimgHeight = pImage->GetHeight();
#endif

	const RECT& c_rRect = pImage->GetRectReference();
	const float texReverseWidth = 1.0f / float(pTexture->GetWidth());
	const float texReverseHeight = 1.0f / float(pTexture->GetHeight());
	float su = c_rRect.left * texReverseWidth;
	float sv = c_rRect.top * texReverseHeight;
	float eu = (c_rRect.left + (c_rRect.right-c_rRect.left)) * texReverseWidth;
	float ev = (c_rRect.top + (c_rRect.bottom-c_rRect.top)) * texReverseHeight;

#if defined(__BL_CLIP_MASK__)
	float v1 = m_v2Position.x - 0.5f;
	float v2 = m_v2Position.y - 0.5f;
	float v3 = m_v2Position.x + fimgWidth - 0.5f;
	float v4 = m_v2Position.y + fimgHeight - 0.5f;

	if (rMask)
	{
		const float v5 = v3 - v1;
		const float v6 = v4 - v2;
		const float v7 = eu - su;
		const float v8 = ev - sv;

		if (v3 < rMask->left)
			return;

		if (v1 < rMask->left)
		{
			const float fCal = rMask->left - v1;
			su += fCal / v5 * v7;
			v1 += fCal;
		}

		if (v4 < rMask->top)
			return;

		if (v2 < rMask->top)
		{
			const float fCal = rMask->top - v2;
			sv += fCal / v6 * v8;
			v2 += fCal;
		}

		if (v1 > rMask->right)
			return;

		if (v1 < rMask->right && v3 > rMask->right)
		{
			const float fCal = v3 - rMask->right;
			v3 -= fCal;
			eu -= fCal / v5 * v7;
		}

		if (v2 > rMask->bottom)
			return;

		if (v2 < rMask->bottom && v4 > rMask->bottom)
		{
			const float fCal = v4 - rMask->bottom;
			ev -= v8 * fCal / v6;
			v4 -= fCal;
		}
	}

	TPDTVertex vertices[4];
	vertices[0].position.x = v1;
	vertices[0].position.y = v2;
	vertices[0].position.z = 0.0f;
	vertices[0].texCoord = TTextureCoordinate(su, sv);
	vertices[0].diffuse = m_DiffuseColor;

	vertices[1].position.x = v3;
	vertices[1].position.y = v2;
	vertices[1].position.z = 0.0f;
	vertices[1].texCoord = TTextureCoordinate(eu, sv);
	vertices[1].diffuse = m_DiffuseColor;

	vertices[2].position.x = v1;
	vertices[2].position.y = v4;
	vertices[2].position.z = 0.0f;
	vertices[2].texCoord = TTextureCoordinate(su, ev);
	vertices[2].diffuse = m_DiffuseColor;

	vertices[3].position.x = v3;
	vertices[3].position.y = v4;
	vertices[3].position.z = 0.0f;
	vertices[3].texCoord = TTextureCoordinate(eu, ev);
	vertices[3].diffuse = m_DiffuseColor;
#else
	TPDTVertex vertices[4];
	vertices[0].position.x = m_v2Position.x - 0.5f;
	vertices[0].position.y = m_v2Position.y - 0.5f;
	vertices[0].position.z = 0.0f;
	vertices[0].texCoord = TTextureCoordinate(su, sv);
	vertices[0].diffuse = m_DiffuseColor;

	vertices[1].position.x = m_v2Position.x + fimgWidth - 0.5f;
	vertices[1].position.y = m_v2Position.y - 0.5f;
	vertices[1].position.z = 0.0f;
	vertices[1].texCoord = TTextureCoordinate(eu, sv);
	vertices[1].diffuse = m_DiffuseColor;

	vertices[2].position.x = m_v2Position.x - 0.5f;
	vertices[2].position.y = m_v2Position.y + fimgHeight - 0.5f;
	vertices[2].position.z = 0.0f;
	vertices[2].texCoord = TTextureCoordinate(su, ev);
	vertices[2].diffuse = m_DiffuseColor;

	vertices[3].position.x = m_v2Position.x + fimgWidth - 0.5f;
	vertices[3].position.y = m_v2Position.y + fimgHeight - 0.5f;
	vertices[3].position.z = 0.0f;
	vertices[3].texCoord = TTextureCoordinate(eu, ev);
	vertices[3].diffuse = m_DiffuseColor;
#endif

#ifdef ENABLE_UI_MOVING
	if (m_bScalePivotCenter)
	{
		vertices[0].texCoord = TTextureCoordinate(eu, sv);
		vertices[1].texCoord = TTextureCoordinate(su, sv);
		vertices[2].texCoord = TTextureCoordinate(eu, ev);
		vertices[3].texCoord = TTextureCoordinate(su, ev);
	}
#endif

#ifdef ENABLE_AUTO_L2R
	if (m_bLeftRightReverse)
	{
		vertices[0].texCoord = TTextureCoordinate(eu, sv);
		vertices[1].texCoord = TTextureCoordinate(su, sv);
		vertices[2].texCoord = TTextureCoordinate(eu, ev);
		vertices[3].texCoord = TTextureCoordinate(su, ev);
	}
#endif

	if (CGraphicBase::SetPDTStream(vertices, 4))
	{
		CGraphicBase::SetDefaultIndexBuffer(CGraphicBase::DEFAULT_IB_FILL_RECT);

		STATEMANAGER.SetTexture(0, pTexture->GetD3DTexture());
		STATEMANAGER.SetTexture(1, nullptr);
		STATEMANAGER.SetFVF(D3DFVF_XYZ|D3DFVF_DIFFUSE|D3DFVF_TEX1);
		STATEMANAGER.DrawIndexedPrimitive(D3DPT_TRIANGLELIST, 0, 4, 0, 2, 0);
	}
	//OLD: STATEMANAGER.DrawIndexedPrimitiveUP(D3DPT_TRIANGLELIST, 0, 4, 2, c_FillRectIndices, D3DFMT_INDEX16, vertices, sizeof(TPDTVertex));
	////////////////////////////////////////////////////////////
}

const CGraphicTexture & CGraphicImageInstance::GetTextureReference() const
{
	return m_roImage->GetTextureReference();
}

CGraphicTexture * CGraphicImageInstance::GetTexturePointer() const
{
	CGraphicImage* pkImage = m_roImage.GetPointer();
	return pkImage ? pkImage->GetTexturePointer() : nullptr;
}

CGraphicImage * CGraphicImageInstance::GetGraphicImagePointer() const
{
	return m_roImage.GetPointer();
}

int CGraphicImageInstance::GetWidth() const
{
	if (IsEmpty())
		return 0;

	return m_roImage->GetWidth();
}

int CGraphicImageInstance::GetHeight() const
{
	if (IsEmpty())
		return 0;

	return m_roImage->GetHeight();
}

void CGraphicImageInstance::SetDiffuseColor(float fr, float fg, float fb, float fa)
{
	m_DiffuseColor.r = fr;
	m_DiffuseColor.g = fg;
	m_DiffuseColor.b = fb;
	m_DiffuseColor.a = fa;
}
void CGraphicImageInstance::SetPosition(float fx, float fy)
{
	m_v2Position.x = fx;
	m_v2Position.y = fy;
}

void CGraphicImageInstance::SetImagePointer(CGraphicImage * pImage)
{
	m_roImage.SetPointer(pImage);

	OnSetImagePointer();
}

void CGraphicImageInstance::ReloadImagePointer(CGraphicImage * pImage)
{
	if (m_roImage.IsNull())
	{
		SetImagePointer(pImage);
		return;
	}

	CGraphicImage * pkImage = m_roImage.GetPointer();

	if (pkImage)
		pkImage->Reload();
}

bool CGraphicImageInstance::IsEmpty() const
{
	if (!m_roImage.IsNull() && !m_roImage->IsEmpty())
		return false;

	return true;
}

bool CGraphicImageInstance::operator == (const CGraphicImageInstance & rhs) const
{
	return (m_roImage.GetPointer() == rhs.m_roImage.GetPointer());
}

DWORD CGraphicImageInstance::Type()
{
	static DWORD s_dwType = GetCRC32("CGraphicImageInstance", strlen("CGraphicImageInstance"));
	return (s_dwType);
}

BOOL CGraphicImageInstance::IsType(DWORD dwType)
{
	return OnIsType(dwType);
}

BOOL CGraphicImageInstance::OnIsType(DWORD dwType)
{
	if (CGraphicImageInstance::Type() == dwType)
		return TRUE;

	return FALSE;
}

void CGraphicImageInstance::OnSetImagePointer()
{
}

void CGraphicImageInstance::Initialize()
{
	m_DiffuseColor.r = m_DiffuseColor.g = m_DiffuseColor.b = m_DiffuseColor.a = 1.0f;
	m_v2Position.x = m_v2Position.y = 0.0f;
#ifdef ENABLE_UI_MOVING
	m_v2Scale.x = m_v2Scale.y = 1.0f;
	m_bScalePivotCenter = false;
#endif
#ifdef ENABLE_AUTO_L2R
	m_bLeftRightReverse = false;
#endif
}

void CGraphicImageInstance::Destroy()
{
	m_roImage.SetPointer(nullptr);
	Initialize();
}

CGraphicImageInstance::CGraphicImageInstance()
{
	Initialize();
}

CGraphicImageInstance::~CGraphicImageInstance()
{
	Destroy();
}

#ifdef ENABLE_UI_MOVING
void CGraphicImageInstance::SetScale(float fx, float fy)
{
	m_v2Scale.x = fx;
	m_v2Scale.y = fy;
}

void CGraphicImageInstance::SetScale(D3DXVECTOR2 v2Scale)
{
	m_v2Scale = v2Scale;
}

void CGraphicImageInstance::SetScalePercent(BYTE byPercent)
{
	m_v2Scale.x *= byPercent;
	m_v2Scale.y *= byPercent;
}

const D3DXVECTOR2& CGraphicImageInstance::GetScale() const
{
	return m_v2Scale;
}

void CGraphicImageInstance::SetScalePivotCenter(bool bScalePivotCenter)
{
	m_bScalePivotCenter = bScalePivotCenter;
}
#endif

#ifdef ENABLE_AUTO_L2R
void CGraphicImageInstance::LeftRightReverse()
{
	m_bLeftRightReverse = true;
}
#endif

