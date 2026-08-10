#pragma once

#include "GrpImage.h"
#include "GrpIndexBuffer.h"
#include "GrpVertexBufferDynamic.h"
#include "Pool.h"

class CGraphicImageInstance
{
	public:
		static DWORD Type();
		BOOL IsType(DWORD dwType);

	public:
		CGraphicImageInstance();
		virtual ~CGraphicImageInstance();

		void Destroy();

#if defined(__BL_CLIP_MASK__)
		void Render(RECT* rMask = nullptr);
#else
		void Render();
#endif

		void SetDiffuseColor(float fr, float fg, float fb, float fa);
		void SetPosition(float fx, float fy);

		void SetImagePointer(CGraphicImage* pImage);
		void ReloadImagePointer(CGraphicImage* pImage);
		bool IsEmpty() const;

		int GetWidth() const;
		int GetHeight() const;

		CGraphicTexture * GetTexturePointer() const;
		const CGraphicTexture &	GetTextureReference() const;
		CGraphicImage * GetGraphicImagePointer() const;

		bool operator == (const CGraphicImageInstance & rhs) const;

	protected:
		void Initialize();

#if defined(__BL_CLIP_MASK__)
		virtual void OnRender(RECT* rMask);
#else
		virtual void OnRender();
#endif
		virtual void OnSetImagePointer();

		virtual BOOL OnIsType(DWORD dwType);

	protected:
		D3DXCOLOR m_DiffuseColor;
		D3DXVECTOR2 m_v2Position;

		CGraphicImage::TRef m_roImage;

	public:
		static void CreateSystem(UINT uCapacity);
		static void DestroySystem();

		static CGraphicImageInstance* New();
		static void Delete(CGraphicImageInstance* pkImgInst);

		static CDynamicPool<CGraphicImageInstance>		ms_kPool;

#ifdef ENABLE_UI_MOVING
public:
	void SetScale(float fx, float fy);
	void SetScale(D3DXVECTOR2 v2Scale);
	const D3DXVECTOR2& GetScale() const;
	void SetScalePercent(BYTE byPercent);
	void SetScalePivotCenter(bool bScalePivotCenter);
protected:
	D3DXVECTOR2 m_v2Scale;
	bool m_bScalePivotCenter;
#endif

#ifdef ENABLE_AUTO_L2R
public:
	void LeftRightReverse();
protected:
	bool m_bLeftRightReverse;
#endif
};

