#include "stdafx.h"
#include "locale_service.h"

#include <cstdarg>

typedef std::map< std::string, std::string > LocaleStringMapType;

// Legacy single-locale map (kept for the non-__LANGUAGE_SYSTEM__ build; the
// multi-language engine below uses localeString_lang[]).
LocaleStringMapType localeString;

int g_iUseLocale = 0;

#ifdef __LANGUAGE_SYSTEM__

// Current per-player language context (set from the desc while dispatching).
BYTE g_iCurrentLang = LANGUAGE_DEFAULT;

// UPPERCASE language codes; must match the runtime files locale_string_XX.txt
std::string arstLocaleStringNames[LANGUAGE_MAX_NUM + 1] =
{
	"AE", "CZ", "DE", "DK", "EN", "ES", "FR", "GR",
	"HU", "IT", "NL", "PL", "PT", "RO", "RU", "TR"
};

LocaleStringMapType localeString_lang[LANGUAGE_MAX_NUM];

void locale_clear()
{
	for (int i = 0; i < LANGUAGE_MAX_NUM; ++i)
		localeString_lang[i].clear();
}

void locale_add_lang(BYTE lang_type, const char * szBaseText, const char * szLangText)
{
	if (lang_type >= LANGUAGE_MAX_NUM)
		return;

	if (!szBaseText || !szLangText)
		return;

	const LocaleStringMapType::const_iterator iter = localeString_lang[lang_type].find(szBaseText);

	if (iter == localeString_lang[lang_type].end())
		localeString_lang[lang_type].emplace(szBaseText, szLangText);
}

const char * locale_find_lang(BYTE lang_type, const char * string)
{
	if (!string || !*string)
		return string;

	if (0 == g_iUseLocale || LC_IsKorea() || LC_IsWE_Korea())
		return string;

	if (lang_type >= LANGUAGE_MAX_NUM)
		lang_type = LANGUAGE_DEFAULT;

	LocaleStringMapType::const_iterator iter = localeString_lang[lang_type].find(string);

	if (iter == localeString_lang[lang_type].end())
	{
		// Fallback: the default language table, then the legacy "@0949" marker.
		if (lang_type != LANGUAGE_DEFAULT)
		{
			iter = localeString_lang[LANGUAGE_DEFAULT].find(string);

			if (iter != localeString_lang[LANGUAGE_DEFAULT].end())
				return iter->second.c_str();
		}

		static char s_line[1024] = "@0949";
		strlcpy(s_line + 5, string, sizeof(s_line) - 5);

		sys_err("LOCALE_ERROR [%d]: \"%s\";", lang_type, string);
		return s_line;
	}

	return iter->second.c_str();
}

const char * locale_find_new_lang(BYTE lang_type, const char * string, ...)
{
	static char s_szBuf[1024 + 1];

	va_list args;
	va_start(args, string);
	vsnprintf(s_szBuf, sizeof(s_szBuf), string, args);
	va_end(args);

	return locale_find_lang(lang_type, s_szBuf);
}

#endif // __LANGUAGE_SYSTEM__

void locale_add(const char **strings)
{
	const LocaleStringMapType::const_iterator iter = localeString.find( strings[0] );

	if( iter == localeString.end() )
	{
		localeString.emplace(strings[0], strings[1]);
	}
}

const char * locale_find(const char *string)
{
#ifdef __LANGUAGE_SYSTEM__
	// Wrapper: resolve with the language of the current player context.
	return locale_find_lang(g_iCurrentLang, string);
#else
	if (0 == g_iUseLocale || LC_IsKorea() || LC_IsWE_Korea())
	{
		return (string);
	}

	const LocaleStringMapType::const_iterator iter = localeString.find( string );

	if( iter == localeString.end() )
	{
		static char s_line[1024] = "@0949";
		strlcpy(s_line + 5, string, sizeof(s_line) - 5);

		sys_err("LOCALE_ERROR: \"%s\";", string);
		return s_line;
	}

	return iter->second.c_str();
#endif
}

const char *quote_find_end(const char *string)
{
	const char  *tmp = string;
	int         quote = 0;

	while (*tmp)
	{
		if (quote && *tmp == '\\' && *(tmp + 1))
		{
			switch (*(tmp + 1))
			{
				case '"':
					tmp += 2;
					continue;
			}
		}
		else if (*tmp == '"')
		{
			quote = !quote;
		}
		else if (!quote && *tmp == ';')
			return (tmp);

		tmp++;
	}

	return (nullptr);
}

char *locale_convert(const char *src, int len)
{
	const char	*tmp;
	int		i, j;
	char	*buf, *dest;
	int		start = 0;
	char	last_char = 0;

	if (!len)
		return nullptr;

	buf = M2_NEW char[len + 1];

	for (j = i = 0, tmp = src, dest = buf; i < len; i++, tmp++)
	{
		if (*tmp == '"')
		{
			if (last_char != '\\')
				start = !start;
			else
				goto ENCODE;
		}
		else if (*tmp == ';')
		{
			if (last_char != '\\' && !start)
				break;
			else
				goto ENCODE;
		}
		else if (start)
		{
ENCODE:
			if (*tmp == '\\' && *(tmp + 1) == 'n')
			{
				*(dest++) = '\n';
				tmp++;
				last_char = '\n';
			}
			else
			{
				*(dest++) = *tmp;
				last_char = *tmp;
			}

			j++;
		}
	}

	if (!j)
	{
		M2_DELETE_ARRAY(buf);
		return nullptr;
	}

	*dest = '\0';
	return (buf);
}

#define NUM_LOCALES 2

// Shared parser: reads "key" "value" pairs and feeds them to the given adder.
static int locale_init_file(BYTE lang_type, const char *filename, void (*adder)(BYTE, const char *, const char *))
{
	FILE        *fp = fopen(filename, "rb");
	char        *buf;
	int		loaded = 0;

	if (!fp) return 0;

	fseek(fp, 0L, SEEK_END);
	int i = ftell(fp);
	fseek(fp, 0L, SEEK_SET);

	i++;

	buf = M2_NEW char[i];

	memset(buf, 0, i);

	fread(buf, i - 1, sizeof(char), fp);

	fclose(fp);

	const char * tmp;
	const char * end;

	char *	strings[NUM_LOCALES];

	if (!buf)
	{
		sys_err("locale_read: no file %s", filename);
		exit(1);
	}

	tmp = buf;

	do
	{
		for (i = 0; i < NUM_LOCALES; i++)
			strings[i] = nullptr;

		if (*tmp == '"')
		{
			for (i = 0; i < NUM_LOCALES; i++)
			{
				if (!(end = quote_find_end(tmp)))
					break;

				strings[i] = locale_convert(tmp, end - tmp);
				tmp = ++end;

				while (*tmp == '\n' || *tmp == '\r' || *tmp == ' ') tmp++;

				if (i + 1 == NUM_LOCALES)
					break;

				if (*tmp != '"')
				{
					sys_err("locale_init: invalid format filename %s", filename);
					break;
				}
			}

			if (strings[0] == nullptr || strings[1] == nullptr)
				break;

			adder(lang_type, strings[0], strings[1]);
			loaded++;

			for (i = 0; i < NUM_LOCALES; i++)
				if (strings[i])
					M2_DELETE_ARRAY(strings[i]);
		}
		else
		{
			tmp = strchr(tmp, '\n');

			if (tmp)
				tmp++;
		}
	}
	while (tmp && *tmp);

	M2_DELETE_ARRAY(buf);

	return loaded;
}

static void locale_add_wrapper(BYTE lang_type, const char * szBase, const char * szLang)
{
	locale_add_lang(lang_type, szBase, szLang);
}

static void locale_add_legacy(BYTE lang_type, const char * szBase, const char * szLang)
{
	const char * szStrings[2] = { szBase, szLang };
	locale_add(szStrings);
}

#ifdef __LANGUAGE_SYSTEM__
int locale_init_lang(BYTE lang_type, const char *filename)
{
	if (lang_type >= LANGUAGE_MAX_NUM)
		return 0;

	return locale_init_file(lang_type, filename, locale_add_wrapper);
}
#endif

void locale_init(const char *filename)
{
	// Legacy single-locale loader (used only by the non-__LANGUAGE_SYSTEM__
	// build; with the Language System on it is compiled but unused).
#ifdef __LANGUAGE_SYSTEM__
	const BYTE bLang = LANGUAGE_DEFAULT;
#else
	const BYTE bLang = 0;
#endif

	locale_init_file(bLang, filename, locale_add_legacy);
}
