#pragma once

#include <map>
#include <string>
#include <unordered_map>

#include "../EterBase/Singleton.h"

// F1 (locale redesign — docs/plans/locale-redesign.md, Client section):
// caché de texto servida por el AUTH (GC_LOCALE 140). El servidor es dueño
// de los nombres: el cliente los pide (CG_LOCALE_REQUEST 132, ver
// CAccountConnector::SendLocaleRequest) y guarda el bundle por dominio.
//
// Fallback chain (cache → pack → vacío) la aplican los CALLERS — aquí solo
// vive el cache; el pack sigue cargado durante la transición (F1 no toca el
// pack loading).
//
// Los dominios numéricos (mob/item/item_desc/skill/map) usan uint32_t como
// clave: el hot path GetMobName(vnum) no convierte el vnum a string en cada
// lookup; las claves ASCII del wire se parsean una vez al recibir el bundle.
// El dominio UI usa claves string (keys de locale_interface.txt).
class CPythonLocale : public CSingleton<CPythonLocale>
{
	public:
		enum ELocaleKind
		{
			LOCALE_KIND_MOB = 0,
			LOCALE_KIND_ITEM,
			LOCALE_KIND_ITEM_DESC,
			LOCALE_KIND_SKILL,
			LOCALE_KIND_MAP,
			LOCALE_KIND_UI,
			LOCALE_KIND_MAX,
		};

	public:
		CPythonLocale();
		virtual ~CPythonLocale();

		// Almacena el idioma y limpia la caché (un bundle nuevo rellena los
		// mapas). El (re)request del bundle lo dispara la capa de red
		// (CAccountConnector::SendLocaleRequest — hot reload futuro).
		void SetLanguage(const char* lang);
		const char* GetLanguage() const { return m_strLanguage.c_str(); }

		// Reensamblado de chunks del GC_LOCALE: appendea el chunk y, cuando
		// bChunkFlag == 0 (final), parsea el buffer completo.
		bool AppendChunk(BYTE bChunkFlag, const BYTE* pChunk, int iChunkLen);
		// Parse del bundle (spec F1): u8 section_count, por sección:
		// u8 kind + u32 count + count × (u16 key_len + key + u16 val_len + val).
		// El bundle es la foto COMPLETA del idioma → reemplaza los mapas.
		bool ParseBundle(const BYTE* pData, int iSize);
		void Clear();

		// Lookups (hot path). nullptr = sin entrada en el bundle (el caller
		// cae al pack / vacío).
		const std::string* GetMobName(uint32_t vnum) const;
		const std::string* GetItemName(uint32_t vnum) const;
		const std::string* GetItemDesc(uint32_t vnum) const;
		const std::string* GetSkillName(uint32_t id) const;
		const std::string* GetMapName(uint32_t id) const;
		const std::string* GetUIText(const char* key) const;

	protected:
		void ParseKeyValue(int kind, const std::string& key, std::string&& val);

	protected:
		std::string m_strLanguage;
		std::string m_strBuffer; // reensamblado de chunks

		std::unordered_map<uint32_t, std::string> m_mobNames;
		std::unordered_map<uint32_t, std::string> m_itemNames;
		std::unordered_map<uint32_t, std::string> m_itemDescs;
		std::unordered_map<uint32_t, std::string> m_skillNames;
		std::unordered_map<uint32_t, std::string> m_mapNames;
		std::map<std::string, std::string> m_uiTexts;
};
