#pragma once

#include "../eterBase/Stl.h"

class CMSWindow
{
	public:
		CMSWindow();

		virtual ~CMSWindow();

		void Destroy();
		bool Create(const char* c_szName, int brush=BLACK_BRUSH, DWORD cs=0, DWORD ws=WS_OVERLAPPEDWINDOW, HICON hIcon= nullptr, int iCursorResource=32512);

		void Show();
		void Hide();

		void SetVisibleMode(bool isVisible);

		void SetPosition(int x, int y) const;
		void SetCenterPosition() const;

		void SetText(const char* c_szText) const;

		void AdjustSize(int width, int height);
		void SetSize(int width, int height) const;

		bool IsVisible() const;
		bool IsActive() const;

		void GetMousePosition(POINT* ppt) const;
		void GetClientRect(RECT* prc) const;
		void GetWindowRect(RECT* prc) const;

		int	GetScreenWidth() const;
		int	GetScreenHeight() const;

		HWND GetWindowHandle() const;
		HINSTANCE GetInstance() const;

		virtual LRESULT	WindowProcedure(HWND hWnd, UINT uiMsg, WPARAM wParam, LPARAM lParam);
		virtual void	OnSize(WPARAM wParam, LPARAM lParam);

	protected:
		const char* RegisterWindowClass(DWORD style, int brush, WNDPROC pfnWndProc, HICON hIcon= nullptr, int iCursorResource=32512) const;

	protected:
		typedef std::set<char*, stl_sz_less> TWindowClassSet;

	protected:
		HWND m_hWnd;
		RECT m_rect;
		bool m_isActive;
		bool m_isVisible;

	protected:
		static TWindowClassSet ms_stWCSet;
		static HINSTANCE ms_hInstance;
};

