#include "StdAfx.h"
#include "../eterBase/Utils.h"
#include "GrpText.h"

CGraphicText::CGraphicText(const char* c_szFileName) : CResource(c_szFileName)
{
}

CGraphicText::~CGraphicText()
{
}

bool CGraphicText::CreateDeviceObjects()
{
	return m_fontTexture.CreateDeviceObjects();
}

void CGraphicText::DestroyDeviceObjects()
{
	m_fontTexture.DestroyDeviceObjects();
}

CGraphicFontTexture* CGraphicText::GetFontTexturePointer()
{
	return &m_fontTexture;
}

CGraphicText::TType CGraphicText::Type()
{
	static TType s_type = StringToType("CGraphicText");
	return s_type;
}

bool CGraphicText::OnLoad(int /*iSize*/, const void* /*c_pvBuf*/)
{
	static char strName[32];
	int size;
	bool bItalic = false;
#ifdef ENABLE_FONT_EX
	bool bBold = false;
	bool bUnderLine = false;
	bool bStrikeOut = false;
#endif

	// format
	const char * p = strrchr(GetFileName(), ':');

	if (p)
	{
		strncpy(strName, GetFileName(), MIN(31, p - GetFileName()));
		++p;

		static char num[8];

		int i = 0;
		while (*p && isdigit(*p))
		{
			num[i++] = *(p++);
		}

		num[i] = '\0';
		bItalic = strchr(p, 'i') != nullptr;
#ifdef ENABLE_FONT_EX
		bBold = strchr(p, 'b') != nullptr;
		bUnderLine = strchr(p, 'u') != nullptr;
		bStrikeOut = strchr(p, 's') != nullptr;
#endif
		size = atoi(num);
	}
	else
	{
		p = strrchr(GetFileName(), '.');

		if (!p)
		{
			assert(!"CGraphicText::OnLoadFromFile there is no extension (ie: .fnt)");
			strName[0] = '\0';
		}
		else
			strncpy(strName, GetFileName(), MIN(31, p - GetFileName()));

		size = 12;
	}

	if (!m_fontTexture.Create(strName, size, bItalic
		#ifdef ENABLE_FONT_EX
		, bBold, bUnderLine, bStrikeOut
		#endif
	))
		return false;

	return true;
}

void CGraphicText::OnClear()
{
	m_fontTexture.Destroy();
}

bool CGraphicText::OnIsEmpty() const
{
	return m_fontTexture.IsEmpty();
}

bool CGraphicText::OnIsType(TType type)
{
	if (CGraphicText::Type() == type)
		return true;

	return CResource::OnIsType(type);
}

