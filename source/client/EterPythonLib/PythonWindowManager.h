#pragma once

namespace UI
{
	class CWindow;

	class CWindowManager : public CSingleton<CWindowManager>
	{
		public:
			typedef std::map<std::string, CWindow *> TLayerContainer;
			typedef std::list<CWindow *> TWindowContainer;
			typedef std::set<CWindow *> TWindowContainer2;
			typedef std::map<int, CWindow *> TKeyCaptureWindowMap;

		public:
			CWindowManager();
			virtual ~CWindowManager();

			void		Destroy();

			float		GetAspect() const;
			void		SetScreenSize(long lWidth, long lHeight);
			void		SetResolution(int hres, int vres);

			void		GetResolution(long & rx, long & ry) const
			{
				rx=m_iHres;
				ry=m_iVres;
			}

			void		SetMouseHandler(PyObject * poMouseHandler);
			long		GetScreenWidth() const { return m_lWidth; }
			long		GetScreenHeight() const { return m_lHeight; }
			void		GetMousePosition(long & rx, long & ry) const;
			BOOL		IsDragging() const;

			CWindow *	GetLockWindow() const { return m_pLockWindow; }
			CWindow *	GetPointWindow() const { return m_pPointWindow; }
			bool		IsFocus() const { return (m_pActiveWindow || m_pLockWindow); }
			bool		IsFocusWindow(CWindow * pWindow) const { return pWindow == m_pActiveWindow; }

			void		SetParent(CWindow * pWindow, CWindow * pParentWindow) const;
			void		SetPickAlways(CWindow * pWindow);

			enum
			{
				WT_NORMAL,
				WT_SLOT,
				WT_GRIDSLOT,
				WT_TEXTLINE,
				WT_MARKBOX,
				WT_IMAGEBOX,
				WT_EXP_IMAGEBOX,
				WT_ANI_IMAGEBOX,
				WT_BUTTON,
				WT_RATIOBUTTON,
				WT_TOGGLEBUTTON,
				WT_DRAGBUTTON,
				WT_BOX,
				WT_BAR,
				WT_LINE,
				WT_BAR3D,
				WT_NUMLINE,
				#ifdef ENABLE_UI_CIRCLE
				WT_CIRCLE,
				#endif
#ifdef ENABLE_UI_MOVING
				WT_MOVE_TEXTLINE,
				WT_MOVE_IMAGEBOX,
				WT_MOVE_SCALEIMAGEBOX,
#endif
			};

			CWindow *	RegisterWindow(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterTypeWindow(PyObject * po, DWORD dwWndType, const char * c_szLayer);

			CWindow *	RegisterSlotWindow(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterGridSlotWindow(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterTextLine(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterMarkBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterImageBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterExpandedImageBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterAniImageBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterButton(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterRadioButton(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterToggleButton(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterDragButton(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterBar(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterLine(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterBar3D(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterNumberLine(PyObject * po, const char * c_szLayer);
			#ifdef ENABLE_UI_CIRCLE
			CWindow *	RegisterCircle(PyObject* po, const char* c_szLayer);
			#endif
#ifdef ENABLE_UI_MOVING
			CWindow *	RegisterMoveTextLine(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterMoveImageBox(PyObject * po, const char * c_szLayer);
			CWindow *	RegisterMoveScaleImageBox(PyObject * po, const char * c_szLayer);
#endif

			void		DestroyWindow(CWindow * pWin);
			void		NotifyDestroyWindow(CWindow * pWindow);

			// Attaching Icon
			BOOL		IsAttaching() const;
			DWORD		GetAttachingType() const;
			DWORD		GetAttachingIndex() const;
			DWORD		GetAttachingSlotNumber() const;
			void		GetAttachingIconSize(BYTE * pbyWidth, BYTE * pbyHeight) const;
			void		AttachIcon(DWORD dwType, DWORD dwIndex, DWORD dwSlotNumber, BYTE byWidth, BYTE byHeight);
			void		DeattachIcon();
			void		SetAttachingFlag(BOOL bFlag);
			// Attaching Icon

			void		OnceIgnoreMouseLeftButtonUpEvent();
			void		LockWindow(CWindow * pWin);
			void		UnlockWindow();

			void		ActivateWindow(CWindow * pWin);
			void		DeactivateWindow();
			CWindow *	GetActivateWindow() const;
			void		SetTop(CWindow * pWin) const;
			void		SetTopUIWindow();
			void		ResetCapture();

			void		Update();
			void		Render() const;

			void		RunMouseMove(long x, long y);
			void		RunMouseLeftButtonDown(long x, long y);
			void		RunMouseLeftButtonUp(long x, long y);
			void		RunMouseLeftButtonDoubleClick(long x, long y);
			void		RunMouseRightButtonDown(long x, long y);
			void		RunMouseRightButtonUp(long x, long y);
			void		RunMouseRightButtonDoubleClick(long x, long y);
			void		RunMouseMiddleButtonDown(long x, long y);
			void		RunMouseMiddleButtonUp(long x, long y);
#ifdef ENABLE_MOUSEWHEEL_EVENT
			bool		RunMouseWheel(short wDelta);
#endif

			void		RunIMEUpdate() const;
			void		RunIMETabEvent();
			void		RunIMEReturnEvent() const;
			void		RunIMEKeyDown(int vkey) const;
			void		RunChangeCodePage() const;
			void		RunOpenCandidate() const;
			void		RunCloseCandidate() const;
			void		RunOpenReading() const;
			void		RunCloseReading() const;

			void		RunKeyDown(int vkey);
			void		RunKeyUp(int vkey);
			void		RunPressEscapeKey() const;
			void		RunPressExitKey() const;

		private:
			void		SetMousePosition(long x, long y);
			CWindow *	__PickWindow(long x, long y);

			CWindow *	__NewWindow(PyObject * po, DWORD dwWndType) const;
			void		__ClearReserveDeleteWindowList();

		private:
			long					m_lWidth;
			long					m_lHeight;

			int						m_iVres;
			int						m_iHres;

			long					m_lMouseX, m_lMouseY;
			long					m_lDragX, m_lDragY;
			long					m_lPickedX, m_lPickedY;

			BOOL					m_bOnceIgnoreMouseLeftButtonUpEventFlag;
			int						m_iIgnoreEndTime;

			// Attaching Icon
			PyObject *				m_poMouseHandler;
			BOOL					m_bAttachingFlag;
			DWORD					m_dwAttachingType;
			DWORD					m_dwAttachingIndex;
			DWORD					m_dwAttachingSlotNumber;
			BYTE					m_byAttachingIconWidth;
			BYTE					m_byAttachingIconHeight;
			// Attaching Icon

			CWindow	*				m_pActiveWindow;
			TWindowContainer		m_ActiveWindowList;
			CWindow *				m_pLockWindow;
			TWindowContainer		m_LockWindowList;
			CWindow	*				m_pPointWindow;
			CWindow	*				m_pLeftCaptureWindow;
			CWindow	*				m_pRightCaptureWindow;
			CWindow *				m_pMiddleCaptureWindow;
			TKeyCaptureWindowMap	m_KeyCaptureWindowMap;
			TWindowContainer2		m_ReserveDeleteWindowList;
			TWindowContainer		m_PickAlwaysWindowList;

			CWindow *				m_pRootWindow;
			TWindowContainer		m_LayerWindowList;
			TLayerContainer			m_LayerWindowMap;

#if defined(__BL_MOUSE_WHEEL_TOP_WINDOW__)
		public:
			bool		OnMouseWheelButtonUp();
			bool		OnMouseWheelButtonDown();
			void		SetWheelTopWindow(CWindow* pWindow);
			void		ClearWheelTopWindow();
		protected:
			CWindow*	m_pTopWindow;
#endif
	};

	PyObject * BuildEmptyTuple();
};

