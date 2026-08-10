#ifndef __INC_METIN2_GAME_LOCALE_H__
#define __INC_METIN2_GAME_LOCALE_H__

#include <string>

extern "C"
{
	void locale_init(const char *filename);
	const char *locale_find(const char *string);

	extern int g_iUseLocale;

#define LC_TEXT(str) locale_find(str)
};

#ifdef __LANGUAGE_SYSTEM__

enum
{
	LANGUAGE_AE = 0,
	LANGUAGE_CZ,
	LANGUAGE_DE,
	LANGUAGE_DK,
	LANGUAGE_EN,
	LANGUAGE_ES,
	LANGUAGE_FR,
	LANGUAGE_GR,
	LANGUAGE_HU,
	LANGUAGE_IT,
	LANGUAGE_NL,
	LANGUAGE_PL,
	LANGUAGE_PT,
	LANGUAGE_RO,
	LANGUAGE_RU,
	LANGUAGE_TR,

	LANGUAGE_MAX_NUM
};

#define LANGUAGE_DEFAULT LANGUAGE_ES

// Language codes in UPPERCASE, matching the runtime files locale_string_XX.txt
extern std::string arstLocaleStringNames[LANGUAGE_MAX_NUM + 1];

// Current per-player language context. Set from the desc while dispatching a
// packet, so the single-arg LC_TEXT(str) resolves to the sender's language.
extern BYTE g_iCurrentLang;

// Multi-language engine. The *_lang names avoid colliding with the legacy C
// functions declared above inside the extern "C" block (no overloading in C).
void locale_clear();
void locale_add_lang(BYTE lang_type, const char * szBaseText, const char * szLangText);
int locale_init_lang(BYTE lang_type, const char * filename);
const char * locale_find_lang(BYTE lang_type, const char * string);
const char * locale_find_new_lang(BYTE lang_type, const char * string, ...);

#define LC_TEXT_LANG(lang_type, str) locale_find_lang(lang_type, str)
#define LC_TEXT_NEW_LANG(lang_type, fmt, ...) locale_find_new_lang(lang_type, fmt, __VA_ARGS__)

#endif // __LANGUAGE_SYSTEM__

#endif
