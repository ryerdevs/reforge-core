#include "StdAfx.h"
#include "PythonLocale.h"

#include "../GameLib/ItemData.h"
#include "../eterLib/Util.h"

// F1 (locale redesign) — ver PythonLocale.h. Parse del wire GC_LOCALE:
// 0x8c + u16 size + u8 chunk_flag + chunk bytes; el bundle completo =
// u8 section_count, por sección: u8 kind + u32 count + count ×
// (u16 key_len + key + u16 val_len + val). Keys ASCII ("101", "173217",
// "INVENTORY"); los dominios numéricos se parsean a uint32_t aquí.

extern DWORD GetDefaultCodePage();

// El wire trae UTF-8 (PG); el pipeline de render legacy decodifica con
// GetDefaultCodePage() (1252 para ES — GrpTextInstance.cpp:202-240). Sin esta
// conversión "Jabalí" UTF-8 (C3 AD) se vería "JabalÃ". Fallback al raw si el
// decode falla (nunca cortar el render por un texto raro).
static std::string Utf8ToDisplay(const std::string& utf8)
{
	if (utf8.empty())
		return utf8;

	const int iSrcLen = (int) utf8.size();
	const int iWideLen = MultiByteToWideChar(CP_UTF8, 0, utf8.c_str(), iSrcLen, nullptr, 0);
	if (iWideLen <= 0)
		return utf8;

	std::vector<wchar_t> wbuf(iWideLen);
	MultiByteToWideChar(CP_UTF8, 0, utf8.c_str(), iSrcLen, wbuf.data(), iWideLen);

	const UINT uCodePage = GetDefaultCodePage();
	const int iDstLen = WideCharToMultiByte(uCodePage, 0, wbuf.data(), iWideLen, nullptr, 0, nullptr, nullptr);
	if (iDstLen <= 0)
		return utf8;

	std::string out(iDstLen, '\0');
	WideCharToMultiByte(uCodePage, 0, wbuf.data(), iWideLen, &out[0], iDstLen, nullptr, nullptr);
	return out;
}

CPythonLocale::CPythonLocale()
{
	// F4 tail: los nombres de ITEM del cache mandan sobre el pack
	// (CItemData::GetName → provider; fallback al pack si no hay entrada).
	CItemData::SetLocaleNameProvider([](DWORD dwVnum) -> const char*
	{
		const std::string* pName = CPythonLocale::Instance().GetItemName(dwVnum);
		return pName ? pName->c_str() : nullptr;
	});
}

CPythonLocale::~CPythonLocale()
{
}

void CPythonLocale::SetLanguage(const char* lang)
{
	m_strLanguage = lang ? lang : "";
	Clear();
}

void CPythonLocale::Clear()
{
	m_strBuffer.clear();
	m_mobNames.clear();
	m_itemNames.clear();
	m_itemDescs.clear();
	m_skillNames.clear();
	m_mapNames.clear();
	m_uiTexts.clear();
}

bool CPythonLocale::AppendChunk(BYTE bChunkFlag, const BYTE* pChunk, int iChunkLen)
{
	if (iChunkLen < 0)
		return false;

	if (iChunkLen > 0)
		m_strBuffer.append(reinterpret_cast<const char*>(pChunk), iChunkLen);

	if (bChunkFlag == 0)
	{
		// Chunk final: el buffer está completo → parsear (y liberar).
		bool ok = ParseBundle(reinterpret_cast<const BYTE*>(m_strBuffer.data()), (int)m_strBuffer.size());
		m_strBuffer.clear();
		return ok;
	}

	return true;
}

bool CPythonLocale::ParseBundle(const BYTE* pData, int iSize)
{
	// Bundle vacío = estado válido (el server puede no tener el idioma): la
	// rama defensiva del caller (AccountConnector) NO debe desconectar.
	if (!pData || iSize < 1)
	{
		Clear();
		return true;
	}

	// El bundle es la foto completa del idioma: reemplaza los mapas.
	Clear();

	const BYTE* p = pData;
	const BYTE* pEnd = pData + iSize;

	const BYTE bSectionCount = *p++;
	int iTotalEntries = 0;

	for (BYTE s = 0; s < bSectionCount; ++s)
	{
		if (pEnd - p < 5) // kind(1) + count(4)
			return false;

		const int kind = *p++;
		uint32_t count;
		memcpy(&count, p, sizeof(count));
		p += sizeof(count);

		if (count > 100000) // defensa contra bundle malformado
			return false;

		for (uint32_t i = 0; i < count; ++i)
		{
			if (pEnd - p < 4) // key_len(2) + val_len(2)
				return false;

			uint16_t keyLen;
			uint16_t valLen;
			memcpy(&keyLen, p, sizeof(keyLen));
			p += sizeof(keyLen);
			memcpy(&valLen, p, sizeof(valLen));
			p += sizeof(valLen);

			if (pEnd - p < keyLen + valLen)
				return false;

			std::string key(reinterpret_cast<const char*>(p), keyLen);
			p += keyLen;
			std::string val(reinterpret_cast<const char*>(p), valLen);
			p += valLen;

			ParseKeyValue(kind, key, std::move(val));
			++iTotalEntries;
		}
	}

	Tracenf("CPythonLocale: bundle lang=%s sections=%u entries=%d",
		m_strLanguage.c_str(), bSectionCount, iTotalEntries);
	return true;
}

void CPythonLocale::ParseKeyValue(int kind, const std::string& key, std::string&& val)
{
	// El wire trae UTF-8 (PG); el render legacy espera el codepage local
	// (GetDefaultCodePage). Conversión al ALMACENAR: los getters devuelven
	// strings listos para render (hot path sin conversión por lookup).
	val = Utf8ToDisplay(val);

	// Claves numéricas del wire (strtoul: no numérica → 0, defensivo).
	const uint32_t nKey = (uint32_t) strtoul(key.c_str(), nullptr, 10);

	switch (kind)
	{
		case LOCALE_KIND_MOB:		m_mobNames[nKey] = std::move(val); break;
		case LOCALE_KIND_ITEM:		m_itemNames[nKey] = std::move(val); break;
		case LOCALE_KIND_ITEM_DESC:	m_itemDescs[nKey] = std::move(val); break;
		case LOCALE_KIND_SKILL:		m_skillNames[nKey] = std::move(val); break;
		case LOCALE_KIND_MAP:		m_mapNames[nKey] = std::move(val); break;
		case LOCALE_KIND_UI:		m_uiTexts[key] = std::move(val); break;
		default:
			// Sección desconocida: se ignora (el wire es aditivo — un server
			// más nuevo puede mandar secciones que este cliente no conoce).
			break;
	}
}

const std::string* CPythonLocale::GetMobName(uint32_t vnum) const
{
	auto it = m_mobNames.find(vnum);
	return it != m_mobNames.end() ? &it->second : nullptr;
}

const std::string* CPythonLocale::GetItemName(uint32_t vnum) const
{
	auto it = m_itemNames.find(vnum);
	return it != m_itemNames.end() ? &it->second : nullptr;
}

const std::string* CPythonLocale::GetItemDesc(uint32_t vnum) const
{
	auto it = m_itemDescs.find(vnum);
	return it != m_itemDescs.end() ? &it->second : nullptr;
}

const std::string* CPythonLocale::GetSkillName(uint32_t id) const
{
	auto it = m_skillNames.find(id);
	return it != m_skillNames.end() ? &it->second : nullptr;
}

const std::string* CPythonLocale::GetMapName(uint32_t id) const
{
	auto it = m_mapNames.find(id);
	return it != m_mapNames.end() ? &it->second : nullptr;
}

const std::string* CPythonLocale::GetUIText(const char* key) const
{
	if (!key)
		return nullptr;

	auto it = m_uiTexts.find(key);
	return it != m_uiTexts.end() ? &it->second : nullptr;
}
