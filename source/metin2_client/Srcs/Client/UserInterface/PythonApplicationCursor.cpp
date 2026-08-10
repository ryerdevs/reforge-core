#include "StdAfx.h"
#include "PythonApplication.h"
#include "resource.h"

bool CPythonApplication::CreateCursors()
{
	m_bCursorVisible = TRUE;
	m_bLiarCursorOn = false;

	const int ResourceID[CURSOR_COUNT] =
	{
		IDC_CURSOR_NORMAL,
		IDC_CURSOR_ATTACK,
		IDC_CURSOR_ATTACK,
		IDC_CURSOR_TALK,
		IDC_CURSOR_NO,
		IDC_CURSOR_PICK,

		IDC_CURSOR_DOOR,
		IDC_CURSOR_CHAIR,
		IDC_CURSOR_CHAIR,			// Magic
		IDC_CURSOR_BUY,				// Buy
		IDC_CURSOR_SELL,			// Sell

		IDC_CURSOR_CAMERA_ROTATE,	// Camera Rotate
		IDC_CURSOR_HSIZE,			// Horizontal Size
		IDC_CURSOR_VSIZE,			// Vertical Size
		IDC_CURSOR_HVSIZE,			// Horizontal & Vertical Size
	};

	m_CursorHandleMap.clear();

	for (int i = 0; i < CURSOR_COUNT; ++i)
	{
		HANDLE hCursor = LoadImage(ms_hInstance, MAKEINTRESOURCE(ResourceID[i]), IMAGE_CURSOR, 32, 32, LR_VGACOLOR);

		if (nullptr == hCursor)
			return false;

		m_CursorHandleMap.insert(TCursorHandleMap::value_type(i, hCursor));
	}

	return true;
}

void CPythonApplication::DestroyCursors()
{
	TCursorHandleMap::iterator itor;
	for (itor = m_CursorHandleMap.begin(); itor != m_CursorHandleMap.end(); ++itor)
	{
		DestroyCursor((HCURSOR) itor->second);
	}
}

void CPythonApplication::SetCursorVisible(BOOL bFlag, bool bLiarCursorOn)
{
	m_bCursorVisible = bFlag;
	m_bLiarCursorOn = bLiarCursorOn;

	if (CURSOR_MODE_HARDWARE == m_iCursorMode)
	{
		int iShowNum;
		if (FALSE == m_bCursorVisible)
		{
			do
			{
				iShowNum = ShowCursor(m_bCursorVisible);
			} while(iShowNum >= 0);
		}
		else
		{
			do
			{
				iShowNum = ShowCursor(m_bCursorVisible);
			} while(iShowNum < 0);
		}
	}
}

BOOL CPythonApplication::GetCursorVisible() const
{
	return m_bCursorVisible;
}

bool CPythonApplication::GetLiarCursorOn() const
{
	return m_bLiarCursorOn;
}

int CPythonApplication::GetCursorMode() const
{
	return m_iCursorMode;
}

BOOL CPythonApplication::__IsContinuousChangeTypeCursor(int iCursorNum) const
{
	switch (iCursorNum)
	{
		case CURSOR_SHAPE_NORMAL:
		case CURSOR_SHAPE_ATTACK:
		case CURSOR_SHAPE_TARGET:
		case CURSOR_SHAPE_MAGIC:
		case CURSOR_SHAPE_BUY:
		case CURSOR_SHAPE_SELL:
			return TRUE;
	}

	return FALSE;
}

BOOL CPythonApplication::SetCursorNum(int iCursorNum)
{
	if (CURSOR_SHAPE_NORMAL == iCursorNum)
	{
		if (!__IsContinuousChangeTypeCursor(m_iCursorNum))
		{
			iCursorNum = m_iContinuousCursorNum;
		}
	}
	else
	{
		if (__IsContinuousChangeTypeCursor(m_iCursorNum))
		{
			m_iContinuousCursorNum = m_iCursorNum;
		}
	}

	if (CURSOR_MODE_HARDWARE == m_iCursorMode)
	{
		const auto itor = m_CursorHandleMap.find(iCursorNum);
		if (m_CursorHandleMap.end() == itor)
			return FALSE;

		const auto hCursor = (HCURSOR)itor->second;

		SetCursor(hCursor);
		m_hCurrentCursor = hCursor;
	}

	m_iCursorNum = iCursorNum;

	PyCallClassMemberFunc(m_poMouseHandler, "ChangeCursor", Py_BuildValue("(i)", m_iCursorNum));

	return TRUE;
}

void CPythonApplication::SetCursorMode(int iMode)
{
	switch (iMode)
	{
		case CURSOR_MODE_HARDWARE:
			m_iCursorMode = CURSOR_MODE_HARDWARE;
			ShowCursor(true);
			break;

		case CURSOR_MODE_SOFTWARE:
			m_iCursorMode = CURSOR_MODE_SOFTWARE;
			SetCursor(nullptr);
			ShowCursor(false);
			break;
	}
}

