#include "StdAfx.h"
#include "GrpTextInstance.h"
#include "StateManager.h"
#include "IME.h"
#include "TextTag.h"
#include "../EterLocale/StringCodec.h"
#include "../EterBase/Utils.h"
#include "../EterLocale/Arabic.h"
#ifdef ENABLE_EMOJI_SYSTEM
#include "ResourceManager.h"
#include <array>
#include <filesystem>
#include <fmt/fmt.h>
#endif

extern DWORD GetDefaultCodePage();

constexpr float c_fFontFeather = 0.5f;

CDynamicPool<CGraphicTextInstance>		CGraphicTextInstance::ms_kPool;

static int gs_mx = 0;
static int gs_my = 0;

static std::wstring gs_hyperlinkText;

void CGraphicTextInstance::Hyperlink_UpdateMousePos(int x, int y)
{
	gs_mx = x;
	gs_my = y;
	gs_hyperlinkText = L"";
}

int CGraphicTextInstance::Hyperlink_GetText(char* buf, int len)
{
	if (gs_hyperlinkText.empty())
		return 0;

	const int codePage = GetDefaultCodePage();

	return Ymir_WideCharToMultiByte(codePage, 0, gs_hyperlinkText.c_str(), gs_hyperlinkText.length(), buf, len, nullptr, nullptr);
}

int CGraphicTextInstance::__DrawCharacter(CGraphicFontTexture * pFontTexture, WORD codePage, wchar_t text, DWORD dwColor)
{
	CGraphicFontTexture::TCharacterInfomation* pInsCharInfo = pFontTexture->GetCharacterInfomation(codePage, text);

	if (pInsCharInfo)
	{
		m_dwColorInfoVector.push_back(dwColor);
		m_pCharInfoVector.push_back(pInsCharInfo);

		m_textWidth += pInsCharInfo->advance;
		m_textHeight = std::max<WORD>(pInsCharInfo->height, m_textHeight);
		return pInsCharInfo->advance;
	}

	return 0;
}

void CGraphicTextInstance::__GetTextPos(DWORD index, float* x, float* y) const
{
	index = std::min<DWORD>(index, m_pCharInfoVector.size());

	float sx = 0;
	float sy = 0;
	float fFontMaxHeight = 0;

	for(DWORD i=0; i<index; ++i)
	{
		if (sx+float(m_pCharInfoVector[i]->width) > m_fLimitWidth)
		{
			sx = 0;
			sy += fFontMaxHeight;
		}

		sx += float(m_pCharInfoVector[i]->advance);
		fFontMaxHeight = std::max<float>(float(m_pCharInfoVector[i]->height), fFontMaxHeight);
	}

	*x = sx;
	*y = sy;
}

bool isNumberic(const char chr)
{
	if (chr >= '0' && chr <= '9')
		return true;
	return false;
}

bool IsValidToken(const char* iter)
{
	return	iter[0]=='@' &&
		isNumberic(iter[1]) &&
		isNumberic(iter[2]) &&
		isNumberic(iter[3]) &&
		isNumberic(iter[4]);
}

const char* FindToken(const char* begin, const char* end)
{
	while(begin < end)
	{
		begin = std::find(begin, end, '@');

		if(end-begin>5 && IsValidToken(begin))
		{
			return begin;
		}
		else
		{
			++begin;
		}
	}

	return end;
}

int ReadToken(const char* token)
{
	const int nRet = (token[1]-'0')*1000 + (token[2]-'0')*100 + (token[3]-'0')*10 + (token[4]-'0');
	if (nRet == 9999)
		return CP_UTF8;
	return nRet;
}

bool CGraphicTextInstance::EmojiPathProcess(const std::wstring& emojiBuffer, SEmoji & kEmoji, int & x, CGraphicFontTexture::TCharacterInfomation*&pSpaceInfo, CGraphicFontTexture* &pFontTexture, const UINT & dataCodePage, const DWORD & dwColor)
{
	char retBuf[1024];
	const int retLen = Ymir_WideCharToMultiByte(GetDefaultCodePage(), 0, emojiBuffer.c_str(), emojiBuffer.length(), retBuf, sizeof(retBuf) - 1, nullptr, nullptr);
	retBuf[retLen] = '\0';

	// list of available paths (they should end with /)
	const static std::array pathList{ "icon/", "icon/emoji/", "" };
	// list of available extensions (they should start with .)
	const static std::array extList{ ".png", ".tga" };
	// get extension path and if it has one
	const auto extPath = std::filesystem::path(retBuf).extension();
	const auto hasExt = std::find(std::begin(extList), std::end(extList), extPath) != std::end(extList);
	// process for paths
	std::string emojiPath;
	for (const auto& pathElem : pathList)
	{
		if (hasExt) // process for known ext
		{
			const auto& tmpPath = fmt::format("{}{}", pathElem, retBuf);
			if (!CResourceManager::Instance().IsFileExist(tmpPath.c_str()))
				continue;
			emojiPath = tmpPath;
		}
		else // otherwise for available extensions
		{
			for (const auto& extElem : extList)
			{
				const auto& tmpPath = fmt::format("{}{}{}", pathElem, retBuf, extElem);
				if (!CResourceManager::Instance().IsFileExist(tmpPath.c_str()))
					continue;
				emojiPath = tmpPath;
			}
		}
		// skip if no path is found
		if (emojiPath.empty())
			continue;

		{
			auto pImage = (CGraphicImage*)CResourceManager::Instance().GetResourcePointer(emojiPath.c_str());
			kEmoji.pInstance = CGraphicImageInstance::New();
			kEmoji.pInstance->SetImagePointer(pImage);

			m_emojiVector.push_back(kEmoji);
			memset(&kEmoji, 0, sizeof(SEmoji));

			for (int i = 0; i < pImage->GetWidth() / (pSpaceInfo->width - 1); ++i)
				x += __DrawCharacter(pFontTexture, dataCodePage, ' ', dwColor);
			if (pImage->GetWidth() % (pSpaceInfo->width - 1) > 1)
				x += __DrawCharacter(pFontTexture, dataCodePage, ' ', dwColor);
			break;
		}
	}
	return true;
}

void CGraphicTextInstance::Update()
{
	if (m_isUpdate)
		return;

	if (m_roText.IsNull())
	{
		Tracef("CGraphicTextInstance::Update - Font has not been set\n");
		return;
	}

	if (m_roText->IsEmpty())
		return;

	CGraphicFontTexture* pFontTexture = m_roText->GetFontTexturePointer();
	if (!pFontTexture)
		return;

	UINT defCodePage = GetDefaultCodePage();

	UINT dataCodePage = defCodePage;

	CGraphicFontTexture::TCharacterInfomation* pSpaceInfo = pFontTexture->GetCharacterInfomation(dataCodePage, ' ');

	int spaceHeight = pSpaceInfo ? pSpaceInfo->height : 12;

	m_pCharInfoVector.clear();
	m_dwColorInfoVector.clear();
	m_hyperlinkVector.clear();
#ifdef ENABLE_EMOJI_SYSTEM
	for (auto& rEmo : m_emojiVector)
	{
		if (rEmo.pInstance)
			CGraphicImageInstance::Delete(rEmo.pInstance);
	}
	m_emojiVector.clear();
#endif

	m_textWidth = 0;
	m_textHeight = spaceHeight;

	/* wstring begin */

	const char* begin = m_stText.c_str();
	const char* end = begin + m_stText.length();

	int wTextMax = (end - begin) * 2;
	auto wText = (wchar_t*)_alloca(sizeof(wchar_t)*wTextMax);

	DWORD dwColor = m_dwTextColor;

	/* wstring end */
	while (begin < end)
	{
		const char * token = FindToken(begin, end);

		int wTextLen = Ymir_MultiByteToWideChar(dataCodePage, 0, begin, token - begin, wText, wTextMax);

		if (m_isSecret)
		{
			for(int i=0; i<wTextLen; ++i)
				__DrawCharacter(pFontTexture, dataCodePage, '*', dwColor);
		}
		else
		{
			if (defCodePage == CP_ARABIC) // ARABIC
			{
				auto wArabicText = (wchar_t*)_alloca(sizeof(wchar_t) * wTextLen);
				int wArabicTextLen = Arabic_MakeShape(wText, wTextLen, wArabicText, wTextLen);

				bool isEnglish = true;
				int nEnglishBase = wArabicTextLen - 1;

				int x = 0;

				int len;
				int hyperlinkStep = 0;
				SHyperlink kHyperlink;
				std::wstring hyperlinkBuffer;
				#ifdef ENABLE_EMOJI_SYSTEM
				SEmoji kEmoji;
				int emojiStep = 0;
				std::wstring emojiBuffer;
				#endif
				int no_hyperlink = 0;

				if (Arabic_IsInSymbol(wArabicText[wArabicTextLen - 1]))
				{
					isEnglish = false;
				}

				int i = 0;
				for (i = wArabicTextLen - 1 ; i >= 0; --i)
				{
					wchar_t wArabicChar = wArabicText[i];

					if (isEnglish)
					{
						//	(2)
						//		or

						if (Arabic_IsInSymbol(wArabicChar) && (
								(i == 0) ||
								(i > 0 &&
									!(Arabic_HasPresentation(wArabicText, i - 1) || Arabic_IsInPresentation(wArabicText[i + 1]))  &&
									wArabicText[i+1] != '|'
								) ||
								wArabicText[i] == '|'
							))//if end.
						{
							// pass
							int temptest = 1;
						}
						else if (Arabic_IsInPresentation(wArabicChar) || Arabic_IsInSymbol(wArabicChar))
						{
							for (int e = i + 1; e <= nEnglishBase;) {
								int ret = GetTextTag(&wArabicText[e], wArabicTextLen - e, len, hyperlinkBuffer);

								if (ret == TEXT_TAG_PLAIN || ret == TEXT_TAG_TAG)
								{
									if (hyperlinkStep == 1)
										hyperlinkBuffer.append(1, wArabicText[e]);
									#ifdef ENABLE_EMOJI_SYSTEM
									else if (emojiStep == 1)
										emojiBuffer.append(1, wArabicText[e]);
									#endif
									else
									{
										int charWidth = __DrawCharacter(pFontTexture, dataCodePage, wArabicText[e], dwColor);
										kHyperlink.ex += charWidth;
										//x += charWidth;

										for (int j = 1; j <= no_hyperlink; j++)
										{
											if(m_hyperlinkVector.size() < j)
												break;

											SHyperlink & tempLink = m_hyperlinkVector[m_hyperlinkVector.size() - j];
											tempLink.ex += charWidth;
											tempLink.sx += charWidth;
										}
									}
								}
								else
								{
									if (ret == TEXT_TAG_COLOR)
										dwColor = htoi(hyperlinkBuffer.c_str(), 8);
									else if (ret == TEXT_TAG_RESTORE_COLOR)
										dwColor = m_dwTextColor;
									else if (ret == TEXT_TAG_HYPERLINK_START)
									{
										hyperlinkStep = 1;
										hyperlinkBuffer = L"";
									}
									else if (ret == TEXT_TAG_HYPERLINK_END)
									{
										if (hyperlinkStep == 1)
										{
											++hyperlinkStep;
											kHyperlink.ex = kHyperlink.sx = 0;
										}
										else
										{
											kHyperlink.text = hyperlinkBuffer;
											m_hyperlinkVector.push_back(kHyperlink);
											no_hyperlink++;

											hyperlinkStep = 0;
											hyperlinkBuffer = L"";
										}
									}
									#ifdef ENABLE_EMOJI_SYSTEM
									else if (ret == TEXT_TAG_EMOJI_START)
									{
										emojiStep = 1;
										emojiBuffer = L"";
									}

									else if (ret == TEXT_TAG_EMOJI_END)
									{
										kEmoji.x = kHyperlink.ex+x;

										EmojiPathProcess(emojiBuffer, kEmoji, x, pSpaceInfo, pFontTexture, dataCodePage, dwColor);

										emojiStep = 0;
										emojiBuffer = L"";
									}
									#endif
								}
								e += len;
							}

							int charWidth = __DrawCharacter(pFontTexture, dataCodePage, Arabic_ConvSymbol(wArabicText[i]), dwColor);
							kHyperlink.ex += charWidth;

							for (int j = 1; j <= no_hyperlink; j++)
							{
								if(m_hyperlinkVector.size() < j)
									break;

								SHyperlink & tempLink = m_hyperlinkVector[m_hyperlinkVector.size() - j];
								tempLink.ex += charWidth;
								tempLink.sx += charWidth;
							}

							isEnglish = false;
						}
					}
					else
					{
						if (Arabic_IsInPresentation(wArabicChar) || Arabic_IsInSymbol(wArabicChar))
						{
							int charWidth = __DrawCharacter(pFontTexture, dataCodePage, Arabic_ConvSymbol(wArabicText[i]), dwColor);
							kHyperlink.ex += charWidth;
							x += charWidth;

							for (int j = 1; j <= no_hyperlink; j++)
							{
								if(m_hyperlinkVector.size() < j)
									break;

								SHyperlink & tempLink = m_hyperlinkVector[m_hyperlinkVector.size() - j];
								tempLink.ex += charWidth;
								tempLink.sx += charWidth;
							}
						}
						else
						{
							nEnglishBase = i;
							isEnglish = true;
						}
					}
				}

				if (isEnglish)
				{
					for (int e = i + 1; e <= nEnglishBase;) {
						int ret = GetTextTag(&wArabicText[e], wArabicTextLen - e, len, hyperlinkBuffer);

						if (ret == TEXT_TAG_PLAIN || ret == TEXT_TAG_TAG)
						{
							if (hyperlinkStep == 1)
								hyperlinkBuffer.append(1, wArabicText[e]);
							#ifdef ENABLE_EMOJI_SYSTEM
							else if (emojiStep == 1)
								emojiBuffer.append(1, wArabicText[e]);
							#endif
							else
							{
								int charWidth = __DrawCharacter(pFontTexture, dataCodePage, wArabicText[e], dwColor);
								kHyperlink.ex += charWidth;

								for (int j = 1; j <= no_hyperlink; j++)
								{
									if(m_hyperlinkVector.size() < j)
										break;

									SHyperlink & tempLink = m_hyperlinkVector[m_hyperlinkVector.size() - j];
									tempLink.ex += charWidth;
									tempLink.sx += charWidth;
								}
							}
						}
						else
						{
							if (ret == TEXT_TAG_COLOR)
								dwColor = htoi(hyperlinkBuffer.c_str(), 8);
							else if (ret == TEXT_TAG_RESTORE_COLOR)
								dwColor = m_dwTextColor;
							else if (ret == TEXT_TAG_HYPERLINK_START)
							{
								hyperlinkStep = 1;
								hyperlinkBuffer = L"";
							}
							else if (ret == TEXT_TAG_HYPERLINK_END)
							{
								if (hyperlinkStep == 1)
								{
									++hyperlinkStep;
									kHyperlink.ex = kHyperlink.sx = 0;
								}
								else
								{
									kHyperlink.text = hyperlinkBuffer;
									m_hyperlinkVector.push_back(kHyperlink);
									no_hyperlink++;

									hyperlinkStep = 0;
									hyperlinkBuffer = L"";
								}
							}
							#ifdef ENABLE_EMOJI_SYSTEM
							else if (ret == TEXT_TAG_EMOJI_START)
							{
								emojiStep = 1;
								emojiBuffer = L"";
							}

							else if (ret == TEXT_TAG_EMOJI_END)
							{
								kEmoji.x = kHyperlink.ex+x;

								EmojiPathProcess(emojiBuffer, kEmoji, x, pSpaceInfo, pFontTexture, dataCodePage, dwColor);

								emojiStep = 0;
								emojiBuffer = L"";
							}
						#endif
						}
						e += len;
					}

				}
			}
			else
			{
				int x = 0;
				int len;
				int hyperlinkStep = 0;
				SHyperlink kHyperlink;
				std::wstring hyperlinkBuffer;
				#ifdef ENABLE_EMOJI_SYSTEM
				SEmoji kEmoji;
				int emojiStep = 0;
				std::wstring emojiBuffer;
				#endif

				for (int i = 0; i < wTextLen; )
				{
					int ret = GetTextTag(&wText[i], wTextLen - i, len, hyperlinkBuffer);

					if (ret == TEXT_TAG_PLAIN || ret == TEXT_TAG_TAG)
					{
						if (hyperlinkStep == 1)
							hyperlinkBuffer.append(1, wText[i]);
						#ifdef ENABLE_EMOJI_SYSTEM
						else if (emojiStep == 1)
							emojiBuffer.append(1, wText[i]);
						#endif
						else
						{
							int charWidth = __DrawCharacter(pFontTexture, dataCodePage, wText[i], dwColor);
							kHyperlink.ex += charWidth;
							x += charWidth;
						}
					}
					else
					{
						if (ret == TEXT_TAG_COLOR)
							dwColor = htoi(hyperlinkBuffer.c_str(), 8);
						else if (ret == TEXT_TAG_RESTORE_COLOR)
							dwColor = m_dwTextColor;
						else if (ret == TEXT_TAG_HYPERLINK_START)
						{
							hyperlinkStep = 1;
							hyperlinkBuffer = L"";
						}
						else if (ret == TEXT_TAG_HYPERLINK_END)
						{
							if (hyperlinkStep == 1)
							{
								++hyperlinkStep;
								kHyperlink.ex = kHyperlink.sx = x;
							}
							else
							{
								kHyperlink.text = hyperlinkBuffer;
								m_hyperlinkVector.push_back(kHyperlink);

								hyperlinkStep = 0;
								hyperlinkBuffer = L"";
							}
						}
						#ifdef ENABLE_EMOJI_SYSTEM
						else if (ret == TEXT_TAG_EMOJI_START)
						{
							emojiStep = 1;
							emojiBuffer = L"";
						}

						else if (ret == TEXT_TAG_EMOJI_END)
						{
							kEmoji.x = x;

							EmojiPathProcess(emojiBuffer, kEmoji, x, pSpaceInfo, pFontTexture, dataCodePage, dwColor);

							emojiStep = 0;
							emojiBuffer = L"";
						}
						#endif
					}
					i += len;
				}
			}
		}

		if (token < end)
		{
			int newCodePage = ReadToken(token);
			dataCodePage = newCodePage;
			begin = token + 5;
		}
		else
		{
			begin = token;
		}
	}

	pFontTexture->UpdateTexture();

	m_isUpdate = true;
}

void CGraphicTextInstance::Render(RECT * pClipRect)
{
	if (!m_isUpdate)
		return;

	CGraphicText* pkText=m_roText.GetPointer();
	if (!pkText)
		return;

	CGraphicFontTexture* pFontTexture = pkText->GetFontTexturePointer();
	if (!pFontTexture)
		return;

	float fStanX = m_v3Position.x;
	float fStanY = m_v3Position.y + 1.0f;

	UINT defCodePage = GetDefaultCodePage();

	if (defCodePage == CP_ARABIC)
	{
		switch (m_hAlign)
		{
			case HORIZONTAL_ALIGN_LEFT:
				fStanX -= m_textWidth;
				break;

			case HORIZONTAL_ALIGN_CENTER:
				fStanX -= float(m_textWidth / 2);
				break;
		}
	}
	else
	{
		switch (m_hAlign)
		{
			case HORIZONTAL_ALIGN_RIGHT:
				fStanX -= m_textWidth;
				break;

			case HORIZONTAL_ALIGN_CENTER:
				fStanX -= float(m_textWidth / 2);
				break;
		}
	}

	switch (m_vAlign)
	{
		case VERTICAL_ALIGN_BOTTOM:
			fStanY -= m_textHeight;
			break;

		case VERTICAL_ALIGN_CENTER:
			fStanY -= float(m_textHeight) / 2.0f;
			break;
	}

	STATEMANAGER.SaveRenderState(D3DRS_SRCBLEND, D3DBLEND_SRCALPHA);
	STATEMANAGER.SaveRenderState(D3DRS_DESTBLEND, D3DBLEND_INVSRCALPHA);
	DWORD dwFogEnable = STATEMANAGER.GetRenderState(D3DRS_FOGENABLE);
	DWORD dwLighting = STATEMANAGER.GetRenderState(D3DRS_LIGHTING);
	STATEMANAGER.SetRenderState(D3DRS_FOGENABLE, FALSE);
	STATEMANAGER.SetRenderState(D3DRS_LIGHTING, FALSE);

	STATEMANAGER.SetFVF(D3DFVF_XYZ|D3DFVF_DIFFUSE|D3DFVF_TEX1);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_COLORARG1,	D3DTA_TEXTURE);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_COLORARG2,	D3DTA_DIFFUSE);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_COLOROP,	D3DTOP_MODULATE);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_ALPHAARG1,	D3DTA_TEXTURE);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_ALPHAARG2,	D3DTA_DIFFUSE);
	STATEMANAGER.SetTextureStageState(0, D3DTSS_ALPHAOP,	D3DTOP_MODULATE);

	{
		constexpr float fFontHalfWeight=1.0f;

		float fCurX, fCurXoutLine;
		float fCurY, fCurYoutline;

		float fFontSx;
		float fFontSy;
		float fFontEx;
		float fFontEy;
		float fFontWidth;
		float fFontHeight;
		float fFontMaxHeight;
		float fFontAdvance;

		CGraphicFontTexture::TCharacterInfomation* pCurCharInfo;

		if (m_isOutline)
		{
			fCurX=fStanX;
			fCurY=fStanY;

			fCurXoutLine=fStanX;
			fCurYoutline=fStanY;

			fFontMaxHeight=0.0f;

			static SVertex akVertex[CGraphicBase::PDT_TEXTLINE_VERTEX_NUM];

			int iActualVertexIdx=0;

			for (int i=0; i<m_pCharInfoVector.size(); ++i)
			{
				if ((iActualVertexIdx+20) >= CGraphicBase::PDT_TEXTLINE_VERTEX_NUM)
				{
					TraceError("Buffer is too small.");
					break;
				}

				pCurCharInfo=m_pCharInfoVector[i];

				fFontWidth=float(pCurCharInfo->width);
				fFontHeight=float(pCurCharInfo->height);
				fFontAdvance=float(pCurCharInfo->advance);

				if ((fCurXoutLine+fFontWidth)-m_v3Position.x > m_fLimitWidth)
				{
					if (m_isMultiLine)
					{
						fCurXoutLine=fStanX;
						fCurYoutline+=fFontMaxHeight;
					}
					else
					{
						break;
					}
				}

#if !defined(__BL_CLIP_MASK__)
				if (pClipRect)
				{
					if (fCurYoutline <= pClipRect->top)
					{
						fCurXoutLine+=fFontAdvance;
						continue;
					}
				}
#endif

				fFontSx=fCurXoutLine-0.5f;
				fFontSy=fCurYoutline-0.5f;
				fFontEx=fFontSx+fFontWidth;
				fFontEy=fFontSy+fFontHeight;

#if defined(__BL_CLIP_MASK__)
				float pleft=pCurCharInfo->left;
				float ptop=pCurCharInfo->top;
				float pright=pCurCharInfo->right;
				float pbottom=pCurCharInfo->bottom;

				if (pClipRect)
				{
					const float v1=pCurCharInfo->right-pCurCharInfo->left;
					const float v2=pCurCharInfo->bottom-pCurCharInfo->top;

					if (fFontEx <= pClipRect->left)
					{
						fCurXoutLine+=fFontAdvance;
						continue;
					}

					if (fFontSx < pClipRect->left)
					{
						const float fCal=pClipRect->left-fFontSx;
						fFontSx+=fCal;
						pleft+=fCal/fFontWidth*v1;
					}

					if (fFontEy <= pClipRect->top)
					{
						fCurXoutLine+=fFontAdvance;
						continue;
					}

					if (fFontSy < pClipRect->top)
					{
						const float fCal=pClipRect->top-fFontSy;
						fFontSy+=fCal;
						ptop+=fCal/fFontHeight*v2;
					}

					if (fFontSx >= pClipRect->right)
					{
						fCurXoutLine+=fFontAdvance;
						continue;
					}

					if (fFontEx > pClipRect->right)
					{
						const float fCal=fFontEx-pClipRect->right;
						fFontEx-=fCal;
						pright-=fCal/fFontWidth*v1;
					}

					if (fFontSy >= pClipRect->bottom)
					{
						fCurXoutLine+=fFontAdvance;
						continue;
					}

					if (fFontEy > pClipRect->bottom)
					{
						const float fCal=fFontEy-pClipRect->bottom;
						fFontEy-=fCal;
						pbottom-=fCal/fFontHeight*v2;
					}
				}

				for (int j=iActualVertexIdx; j<(iActualVertexIdx+16); j+=4)
				{
					akVertex[j].u=pleft;
					akVertex[j].v=ptop;
					akVertex[j+1].u=pleft;
					akVertex[j+1].v=pbottom;
					akVertex[j+2].u=pright;
					akVertex[j+2].v=ptop;
					akVertex[j+3].u=pright;
					akVertex[j+3].v=pbottom;

					akVertex[j].color=akVertex[j+1].color=akVertex[j+2].color=akVertex[j+3].color=m_dwOutLineColor;
				}
#else
				for (int j=iActualVertexIdx; j<(iActualVertexIdx+16); j+=4)
				{
					akVertex[j].u=pCurCharInfo->left;
					akVertex[j].v=pCurCharInfo->top;
					akVertex[j+1].u=pCurCharInfo->left;
					akVertex[j+1].v=pCurCharInfo->bottom;
					akVertex[j+2].u=pCurCharInfo->right;
					akVertex[j+2].v=pCurCharInfo->top;
					akVertex[j+3].u=pCurCharInfo->right;
					akVertex[j+3].v=pCurCharInfo->bottom;

					akVertex[j].color=akVertex[j+1].color=akVertex[j+2].color=akVertex[j+3].color=m_dwOutLineColor;
				}
#endif

				float feather=0.0f;

				akVertex[iActualVertexIdx+0].y=fFontSy-feather;
				akVertex[iActualVertexIdx+1].y=fFontEy+feather;
				akVertex[iActualVertexIdx+2].y=fFontSy-feather;
				akVertex[iActualVertexIdx+3].y=fFontEy+feather;

				akVertex[iActualVertexIdx+0].x=fFontSx-fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+1].x=fFontSx-fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+2].x=fFontEx-fFontHalfWeight+feather;
				akVertex[iActualVertexIdx+3].x=fFontEx-fFontHalfWeight+feather;

				akVertex[iActualVertexIdx+0].z=m_v3Position.z;
				akVertex[iActualVertexIdx+1].z=m_v3Position.z;
				akVertex[iActualVertexIdx+2].z=m_v3Position.z;
				akVertex[iActualVertexIdx+3].z=m_v3Position.z;

				akVertex[iActualVertexIdx+4].y=fFontSy-feather;
				akVertex[iActualVertexIdx+5].y=fFontEy+feather;
				akVertex[iActualVertexIdx+6].y=fFontSy-feather;
				akVertex[iActualVertexIdx+7].y=fFontEy+feather;

				akVertex[iActualVertexIdx+4].x=fFontSx+fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+5].x=fFontSx+fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+6].x=fFontEx+fFontHalfWeight+feather;
				akVertex[iActualVertexIdx+7].x=fFontEx+fFontHalfWeight+feather;

				akVertex[iActualVertexIdx+4].z=m_v3Position.z;
				akVertex[iActualVertexIdx+5].z=m_v3Position.z;
				akVertex[iActualVertexIdx+6].z=m_v3Position.z;
				akVertex[iActualVertexIdx+7].z=m_v3Position.z;

				akVertex[iActualVertexIdx+8].x=fFontSx-feather;
				akVertex[iActualVertexIdx+9].x=fFontSx-feather;
				akVertex[iActualVertexIdx+10].x=fFontEx+feather;
				akVertex[iActualVertexIdx+11].x=fFontEx+feather;

				akVertex[iActualVertexIdx+8].y=fFontSy-fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+9].y=fFontEy-fFontHalfWeight+feather;
				akVertex[iActualVertexIdx+10].y=fFontSy-fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+11].y=fFontEy-fFontHalfWeight+feather;

				akVertex[iActualVertexIdx+8].z=m_v3Position.z;
				akVertex[iActualVertexIdx+9].z=m_v3Position.z;
				akVertex[iActualVertexIdx+10].z=m_v3Position.z;
				akVertex[iActualVertexIdx+11].z=m_v3Position.z;

				akVertex[iActualVertexIdx+12].x=fFontSx-feather;
				akVertex[iActualVertexIdx+13].x=fFontSx-feather;
				akVertex[iActualVertexIdx+14].x=fFontEx+feather;
				akVertex[iActualVertexIdx+15].x=fFontEx+feather;

				akVertex[iActualVertexIdx+12].y=fFontSy+fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+13].y=fFontEy+fFontHalfWeight+feather;
				akVertex[iActualVertexIdx+14].y=fFontSy+fFontHalfWeight-feather;
				akVertex[iActualVertexIdx+15].y=fFontEy+fFontHalfWeight+feather;

				akVertex[iActualVertexIdx+12].z=m_v3Position.z;
				akVertex[iActualVertexIdx+13].z=m_v3Position.z;
				akVertex[iActualVertexIdx+14].z=m_v3Position.z;
				akVertex[iActualVertexIdx+15].z=m_v3Position.z;

				fCurXoutLine+=fFontAdvance;

				iActualVertexIdx+=16;
			}

			int iSize=iActualVertexIdx;
			int iDrawSize=iSize >= 2 ? (iSize-2) : 0;

			if (iDrawSize > 0 && iActualVertexIdx > 0)
			{
				if (m_pCharInfoVector.size() > 0)
				{
					pFontTexture->SelectTexture(m_pCharInfoVector[0]->index);
					STATEMANAGER.SetTexture(0, pFontTexture->GetD3DTexture());
				}
				if (CGraphicBase::SetPDTTextLineStream((SPDTVertexRaw*)akVertex, iSize))
					STATEMANAGER.DrawPrimitive(D3DPT_TRIANGLESTRIP, 0, iDrawSize);
			}
		}

		fCurX=fStanX;
		fCurY=fStanY;
		fFontMaxHeight=0.0f;

		static SVertex akVertex[CGraphicBase::PDT_TEXTLINE_VERTEX_NUM];

		int iActualVertexIdx=0;

		for (int i=0; i<m_pCharInfoVector.size(); ++i)
		{
			if ((iActualVertexIdx+4) >= CGraphicBase::PDT_TEXTLINE_VERTEX_NUM)
			{
				TraceError("Buffer is too small.");
				break;
			}

			pCurCharInfo=m_pCharInfoVector[i];

			fFontWidth=float(pCurCharInfo->width);
			fFontHeight=float(pCurCharInfo->height);
			fFontMaxHeight=std::max<float>(fFontHeight, pCurCharInfo->height);
			fFontAdvance=float(pCurCharInfo->advance);

			if ((fCurX+fFontWidth)-m_v3Position.x > m_fLimitWidth)
			{
				if (m_isMultiLine)
				{
					fCurX=fStanX;
					fCurY+=fFontMaxHeight;
				}
				else
				{
					break;
				}
			}

#if !defined(__BL_CLIP_MASK__)
			if (pClipRect)
			{
				if (fCurY <= pClipRect->top)
				{
					fCurX+=fFontAdvance;
					continue;
				}
			}
#endif

			fFontSx=fCurX-0.5f;
			fFontSy=fCurY-0.5f;
			fFontEx=fFontSx+fFontWidth;
			fFontEy=fFontSy+fFontHeight;

#if defined(__BL_CLIP_MASK__)
			float pleft=pCurCharInfo->left;
			float ptop=pCurCharInfo->top;
			float pright=pCurCharInfo->right;
			float pbottom=pCurCharInfo->bottom;

			if (pClipRect)
			{
				const float v1=pCurCharInfo->right-pCurCharInfo->left;
				const float v2=pCurCharInfo->bottom-pCurCharInfo->top;

				if (fFontEx <= pClipRect->left)
				{
					fCurX+=fFontAdvance;
					continue;
				}

				if (fFontSx < pClipRect->left)
				{
					const float fCal=pClipRect->left-fFontSx;
					fFontSx+=fCal;
					pleft+=fCal/fFontWidth*v1;
				}

				if (fFontEy <= pClipRect->top)
				{
					fCurX+=fFontAdvance;
					continue;
				}

				if (fFontSy < pClipRect->top)
				{
					const float fCal=pClipRect->top-fFontSy;
					fFontSy+=fCal;
					ptop+=fCal/fFontHeight*v2;
				}

				if (fFontSx >= pClipRect->right)
				{
					fCurX+=fFontAdvance;
					continue;
				}

				if (fFontEx > pClipRect->right)
				{
					const float fCal=fFontEx-pClipRect->right;
					fFontEx-=fCal;
					pright-=fCal/fFontWidth*v1;
				}

				if (fFontSy >= pClipRect->bottom)
				{
					fCurX+=fFontAdvance;
					continue;
				}

				if (fFontEy > pClipRect->bottom)
				{
					const float fCal=fFontEy-pClipRect->bottom;
					fFontEy-=fCal;
					pbottom-=fCal/fFontHeight*v2;
				}
			}

			akVertex[iActualVertexIdx].x=fFontSx;
			akVertex[iActualVertexIdx].y=fFontSy;
			akVertex[iActualVertexIdx].z=m_v3Position.z;

			akVertex[iActualVertexIdx].u=pleft;
			akVertex[iActualVertexIdx].v=ptop;

			akVertex[iActualVertexIdx+1].x=fFontSx;
			akVertex[iActualVertexIdx+1].y=fFontEy;
			akVertex[iActualVertexIdx+1].z=m_v3Position.z;

			akVertex[iActualVertexIdx+1].u=pleft;
			akVertex[iActualVertexIdx+1].v=pbottom;

			akVertex[iActualVertexIdx+2].x=fFontEx;
			akVertex[iActualVertexIdx+2].y=fFontSy;
			akVertex[iActualVertexIdx+2].z=m_v3Position.z;

			akVertex[iActualVertexIdx+2].u=pright;
			akVertex[iActualVertexIdx+2].v=ptop;

			akVertex[iActualVertexIdx+3].x=fFontEx;
			akVertex[iActualVertexIdx+3].y=fFontEy;
			akVertex[iActualVertexIdx+3].z=m_v3Position.z;

			akVertex[iActualVertexIdx+3].u=pright;
			akVertex[iActualVertexIdx+3].v=pbottom;
#else
			akVertex[iActualVertexIdx].x=fFontSx;
			akVertex[iActualVertexIdx].y=fFontSy;
			akVertex[iActualVertexIdx].z=m_v3Position.z;

			akVertex[iActualVertexIdx].u=pCurCharInfo->left;
			akVertex[iActualVertexIdx].v=pCurCharInfo->top;

			akVertex[iActualVertexIdx+1].x=fFontSx;
			akVertex[iActualVertexIdx+1].y=fFontEy;
			akVertex[iActualVertexIdx+1].z=m_v3Position.z;

			akVertex[iActualVertexIdx+1].u=pCurCharInfo->left;
			akVertex[iActualVertexIdx+1].v=pCurCharInfo->bottom;

			akVertex[iActualVertexIdx+2].x=fFontEx;
			akVertex[iActualVertexIdx+2].y=fFontSy;
			akVertex[iActualVertexIdx+2].z=m_v3Position.z;

			akVertex[iActualVertexIdx+2].u=pCurCharInfo->right;
			akVertex[iActualVertexIdx+2].v=pCurCharInfo->top;

			akVertex[iActualVertexIdx+3].x=fFontEx;
			akVertex[iActualVertexIdx+3].y=fFontEy;
			akVertex[iActualVertexIdx+3].z=m_v3Position.z;

			akVertex[iActualVertexIdx+3].u=pCurCharInfo->right;
			akVertex[iActualVertexIdx+3].v=pCurCharInfo->bottom;
#endif

			akVertex[iActualVertexIdx+0].color=akVertex[iActualVertexIdx+1].color=akVertex[iActualVertexIdx+2].color=akVertex[iActualVertexIdx+3].color=m_dwColorInfoVector[i];

			iActualVertexIdx+=4;

			fCurX+=fFontAdvance;
		}

		int iSize=iActualVertexIdx;
		int iDrawSize=iSize >= 2 ? (iSize-2) : 0;

		if (iDrawSize > 0 && iActualVertexIdx > 0)
		{
			if (m_pCharInfoVector.size() > 0)
			{
				pFontTexture->SelectTexture(m_pCharInfoVector[0]->index);
				STATEMANAGER.SetTexture(0, pFontTexture->GetD3DTexture());
			}
			if (CGraphicBase::SetPDTTextLineStream((SPDTVertexRaw*)akVertex, iSize))
				STATEMANAGER.DrawPrimitive(D3DPT_TRIANGLESTRIP, 0, iDrawSize);
		}
	}

	if (m_isCursor)
	{
		// Draw Cursor
		float sx, sy, ex, ey;
		TDiffuse diffuse;

		int curpos = CIME::GetCurPos();
		int compend = curpos + CIME::GetCompLen();

		__GetTextPos(curpos, &sx, &sy);

		// If Composition
		if(curpos<compend)
		{
			diffuse = 0x7fffffff;
			__GetTextPos(compend, &ex, &sy);
		}
		else
		{
			diffuse = 0xffffffff;
			ex = sx + 2;
		}

		// FOR_ARABIC_ALIGN
		if (defCodePage == CP_ARABIC)
		{
			sx += m_v3Position.x - m_textWidth;
			ex += m_v3Position.x - m_textWidth;
			sy += m_v3Position.y;
			ey = sy + m_textHeight;
		}
		else
		{
			sx += m_v3Position.x;
			sy += m_v3Position.y;
			ex += m_v3Position.x;
			ey = sy + m_textHeight;
		}

		switch (m_vAlign)
		{
			case VERTICAL_ALIGN_BOTTOM:
				sy -= m_textHeight;
				break;

			case VERTICAL_ALIGN_CENTER:
				sy -= float(m_textHeight) / 2.0f;
				break;
		}

#if defined(__BL_CLIP_MASK__)
		if (pClipRect)
		{
			if (sx < pClipRect->left)
				sx += pClipRect->left - sx;

			if (sy < pClipRect->top)
				sy += pClipRect->top - sy;

			if (ex > pClipRect->right)
				ex -= ex - pClipRect->right;

			if (ey > pClipRect->bottom)
				ey -= ey - pClipRect->bottom;
		}
#endif

		TPDTVertex vertices[4];
		vertices[0].diffuse = diffuse;
		vertices[1].diffuse = diffuse;
		vertices[2].diffuse = diffuse;
		vertices[3].diffuse = diffuse;
		vertices[0].position = TPosition(sx, sy, 0.0f);
		vertices[1].position = TPosition(ex, sy, 0.0f);
		vertices[2].position = TPosition(sx, ey, 0.0f);
		vertices[3].position = TPosition(ex, ey, 0.0f);

		STATEMANAGER.SetTexture(0, nullptr);

		CGraphicBase::SetDefaultIndexBuffer(CGraphicBase::DEFAULT_IB_FILL_RECT);
		if (CGraphicBase::SetPDTStream(vertices, 4))
			STATEMANAGER.DrawIndexedPrimitive(D3DPT_TRIANGLELIST, 0, 4, 0, 2, 0);

		int ulbegin = CIME::GetULBegin();
		int ulend = CIME::GetULEnd();

		if(ulbegin < ulend)
		{
			__GetTextPos(curpos+ulbegin, &sx, &sy);
			__GetTextPos(curpos+ulend, &ex, &sy);

			sx += m_v3Position.x;
			sy += m_v3Position.y + m_textHeight;
			ex += m_v3Position.x;
			ey = sy + 2;

#if defined(__BL_CLIP_MASK__)
			if (pClipRect)
			{
				if (sx < pClipRect->left)
					sx += pClipRect->left - sx;

				if (sy < pClipRect->top)
					sy += pClipRect->top - sy;

				if (ex > pClipRect->right)
					ex -= ex - pClipRect->right;

				if (ey > pClipRect->bottom)
					ey -= ey - pClipRect->bottom;
			}
#endif

			vertices[0].diffuse = 0xFFFF0000;
			vertices[1].diffuse = 0xFFFF0000;
			vertices[2].diffuse = 0xFFFF0000;
			vertices[3].diffuse = 0xFFFF0000;
			vertices[0].position = TPosition(sx, sy, 0.0f);
			vertices[1].position = TPosition(ex, sy, 0.0f);
			vertices[2].position = TPosition(sx, ey, 0.0f);
			vertices[3].position = TPosition(ex, ey, 0.0f);

			STATEMANAGER.DrawIndexedPrimitiveUP(D3DPT_TRIANGLELIST, 0, 4, 2, c_FillRectIndices, D3DFMT_INDEX16, vertices, sizeof(TPDTVertex));
		}
	}

	STATEMANAGER.RestoreRenderState(D3DRS_SRCBLEND);
	STATEMANAGER.RestoreRenderState(D3DRS_DESTBLEND);

	STATEMANAGER.SetRenderState(D3DRS_FOGENABLE, dwFogEnable);
	STATEMANAGER.SetRenderState(D3DRS_LIGHTING, dwLighting);

	if (!m_hyperlinkVector.empty())
	{
		int lx = gs_mx - m_v3Position.x;
		int ly = gs_my - m_v3Position.y;

		if (GetDefaultCodePage() == CP_ARABIC)
		{
			lx = -lx;
			ly = -ly + m_textHeight;
		}

		if (lx >= 0 && ly >= 0 && lx < m_textWidth && ly < m_textHeight)
		{
			auto it = m_hyperlinkVector.begin();

			while (it != m_hyperlinkVector.end())
			{
				SHyperlink & link = *it++;
				if (lx >= link.sx && lx < link.ex)
				{
					gs_hyperlinkText = link.text;
					break;
				}
			}
		}
	}

#ifdef ENABLE_EMOJI_SYSTEM
	if (m_emojiVector.empty() == false)
	{
		for(auto& rEmo : m_emojiVector)
		{
			if (rEmo.pInstance)
			{
				rEmo.pInstance->SetPosition(fStanX + rEmo.x, (fStanY + 7.0) - (rEmo.pInstance->GetHeight() / 2));
				#if defined(__BL_CLIP_MASK__)
				rEmo.pInstance->Render(pClipRect);
				#else
				rEmo.pInstance->Render();
				#endif
			}
		}
	}
#endif
}

void CGraphicTextInstance::CreateSystem(UINT uCapacity)
{
	ms_kPool.Create(uCapacity);
}

void CGraphicTextInstance::DestroySystem()
{
	ms_kPool.Destroy();
}

CGraphicTextInstance* CGraphicTextInstance::New()
{
	return ms_kPool.Alloc();
}

void CGraphicTextInstance::Delete(CGraphicTextInstance* pkInst)
{
	pkInst->Destroy();
	ms_kPool.Free(pkInst);
}

void CGraphicTextInstance::ShowCursor()
{
	m_isCursor = true;
}

void CGraphicTextInstance::HideCursor()
{
	m_isCursor = false;
}

void CGraphicTextInstance::ShowOutLine()
{
	m_isOutline = true;
}

void CGraphicTextInstance::HideOutLine()
{
	m_isOutline = false;
}

void CGraphicTextInstance::SetColor(DWORD color)
{
	if (m_dwTextColor != color)
	{
		for (int i = 0; i < m_pCharInfoVector.size(); ++i)
			if (m_dwColorInfoVector[i] == m_dwTextColor)
				m_dwColorInfoVector[i] = color;

		m_dwTextColor = color;
	}
}

void CGraphicTextInstance::SetColor(float r, float g, float b, float a)
{
	SetColor(D3DXCOLOR(r, g, b, a));
}

void CGraphicTextInstance::SetOutLineColor(DWORD color)
{
	m_dwOutLineColor=color;
}

void CGraphicTextInstance::SetOutLineColor(float r, float g, float b, float a)
{
	m_dwOutLineColor=D3DXCOLOR(r, g, b, a);
}

void CGraphicTextInstance::SetSecret(bool Value)
{
	m_isSecret = Value;
}

void CGraphicTextInstance::SetOutline(bool Value)
{
	m_isOutline = Value;
}

void CGraphicTextInstance::SetFeather(bool Value)
{
	if (Value)
	{
		m_fFontFeather = c_fFontFeather;
	}
	else
	{
		m_fFontFeather = 0.0f;
	}
}

void CGraphicTextInstance::SetMultiLine(bool Value)
{
	m_isMultiLine = Value;
}

void CGraphicTextInstance::SetHorizonalAlign(int hAlign)
{
	m_hAlign = hAlign;
}

void CGraphicTextInstance::SetVerticalAlign(int vAlign)
{
	m_vAlign = vAlign;
}

void CGraphicTextInstance::SetMax(int iMax)
{
	m_iMax = iMax;
}

void CGraphicTextInstance::SetLimitWidth(float fWidth)
{
	m_fLimitWidth = fWidth;
}

void CGraphicTextInstance::SetValueString(const string& c_stValue)
{
	if (0 == m_stText.compare(c_stValue))
		return;

	m_stText = c_stValue;
	m_isUpdate = false;
}

void CGraphicTextInstance::SetValue(const char* c_szText, size_t len)
{
	if (0 == m_stText.compare(c_szText))
		return;

	m_stText = c_szText;
	m_isUpdate = false;
}

void CGraphicTextInstance::SetPosition(float fx, float fy, float fz)
{
	m_v3Position.x = fx;
	m_v3Position.y = fy;
	m_v3Position.z = fz;
}

void CGraphicTextInstance::SetTextPointer(CGraphicText* pText)
{
	m_roText = pText;
}

const std::string & CGraphicTextInstance::GetValueStringReference()
{
	return m_stText;
}

WORD CGraphicTextInstance::GetTextLineCount()
{
	CGraphicFontTexture::TCharacterInfomation* pCurCharInfo;
	CGraphicFontTexture::TPCharacterInfomationVector::iterator itor;

	float fx = 0.0f;
	WORD wLineCount = 1;
	for (itor=m_pCharInfoVector.begin(); itor!=m_pCharInfoVector.end(); ++itor)
	{
		pCurCharInfo = *itor;

		const float fFontWidth=float(pCurCharInfo->width);
		const float fFontAdvance=float(pCurCharInfo->advance);
		//float fFontHeight=float(pCurCharInfo->height);

		if (fx+fFontWidth > m_fLimitWidth)
		{
			fx = 0.0f;
			++wLineCount;
		}

		fx += fFontAdvance;
	}

	return wLineCount;
}

void CGraphicTextInstance::GetTextSize(int* pRetWidth, int* pRetHeight) const
{
	*pRetWidth = m_textWidth;
	*pRetHeight = m_textHeight;
}

int CGraphicTextInstance::PixelPositionToCharacterPosition(int iPixelPosition) const
{
	int icurPosition = 0;
	for (int i = 0; i < (int)m_pCharInfoVector.size(); ++i)
	{
		const CGraphicFontTexture::TCharacterInfomation* pCurCharInfo = m_pCharInfoVector[i];
		icurPosition += pCurCharInfo->width;

		if (iPixelPosition < icurPosition)
			return i;
	}

	return -1;
}

int CGraphicTextInstance::GetHorizontalAlign() const
{
	return m_hAlign;
}

void CGraphicTextInstance::__Initialize()
{
	m_roText = nullptr;

	m_hAlign = HORIZONTAL_ALIGN_LEFT;
	m_vAlign = VERTICAL_ALIGN_TOP;

	m_iMax = 0;
	m_fLimitWidth = 1600.0f;

	m_isCursor = false;
	m_isSecret = false;
	m_isMultiLine = false;

	m_isOutline = false;
	m_fFontFeather = c_fFontFeather;

	m_isUpdate = false;

	m_textWidth = 0;
	m_textHeight = 0;

	m_v3Position.x = m_v3Position.y = m_v3Position.z = 0.0f;

	m_dwOutLineColor=0xff000000;
}

void CGraphicTextInstance::Destroy()
{
	m_stText="";
	m_pCharInfoVector.clear();
	m_dwColorInfoVector.clear();
	m_hyperlinkVector.clear();
#ifdef ENABLE_EMOJI_SYSTEM
	for (const auto & rEmo : m_emojiVector)
	{
		if (rEmo.pInstance)
			CGraphicImageInstance::Delete(rEmo.pInstance);
	}
	m_emojiVector.clear();
#endif

	__Initialize();
}

CGraphicTextInstance::CGraphicTextInstance()
{
	__Initialize();
}

CGraphicTextInstance::~CGraphicTextInstance()
{
	Destroy();
}

