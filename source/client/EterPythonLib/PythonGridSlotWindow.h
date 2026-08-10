#pragma once

#include "PythonSlotWindow.h"

namespace UI
{
	class CGridSlotWindow : public CSlotWindow
	{
		MAKE_UI_WINDOW_TYPE_EX2(CGridSlotWindow, CSlotWindow)

		public:
			CGridSlotWindow(PyObject * ppyObject);
			virtual ~CGridSlotWindow();

			void Destroy();

			void ArrangeGridSlot(DWORD dwStartIndex, DWORD dwxCount, DWORD dwyCount, int ixSlotSize, int iySlotSize, int ixTemporarySize, int iyTemporarySize);

		protected:
			void __Initialize();

			BOOL GetPickedSlotPointer(TSlot ** ppSlot);
			BOOL GetPickedSlotList(int iWidth, int iHeight, std::list<TSlot*> * pSlotPointerList);
			BOOL GetGridSlotPointer(int ix, int iy, TSlot ** ppSlot) const;
			BOOL GetPickedGridSlotPosition(int ixLocal, int iyLocal, int * pix, int * piy) const;
			BOOL CheckMoving(DWORD dwSlotNumber, DWORD dwItemIndex, const std::list<TSlot*> & c_rSlotList) const;

			//BOOL OnIsType(DWORD dwType);

			void OnRefreshSlot();
			void OnRenderPickingSlot();

		protected:
			DWORD m_dwxCount;
			DWORD m_dwyCount;

			std::vector<TSlot *> m_SlotVector;
	};
};

