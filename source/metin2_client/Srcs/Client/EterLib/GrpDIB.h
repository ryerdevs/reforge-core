#pragma once

class CGraphicDib
{
	public:
		CGraphicDib();
		virtual ~CGraphicDib();

		void Destroy();
		bool Create(HDC hDC, int width, int height);

		void SetBkMode(int iBkMode) const;
		void TextOut(int ix, int iy, const char * c_szText) const;
		void Put(HDC hDC, int x, int y) const;

		int GetWidth() const;
		int GetHeight() const;

		void* GetPointer() const;

		HDC GetDCHandle() const;

	protected:
		void Initialize();

	protected:
		HDC			m_hDC;
		HBITMAP		m_hBmp;
		BITMAPINFO	m_bmi;

		int			m_width;
		int			m_height;

		void *		m_pvBuf;
};

