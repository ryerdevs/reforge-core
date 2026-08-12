//! Parser puros del importer F1 (plan `docs/plans/locale-redesign.md`):
//! formato de los dumps de nombres, del pack (itemdesc/skilldesc/
//! locale_interface) y de los archivos del runtime (locale_string_XX.txt,
//! map index/Setting/Town). Sin IO — los tests usan fixtures inline.
//!
//! Parity documentada con los loaders C++/Python reales:
//! - `parse_locale_string` — `locale.cpp` (`locale_init_file` +
//!   `quote_find_end` + `locale_convert`): pares `"key";` `"value";` con
//!   escapes `\"`/`\n` y `;` terminador fuera de comillas; una entrada
//!   malformada DETIENE el parseo (parity `break`).
//! - `parse_itemdesc` / `parse_skilldesc` — `SplitLineByTab`
//!   (FileLoader.cpp:13-36) + `__SnapString` (ItemManager.cpp:178-195) /
//!   `RegisterSkillDesc` (PythonSkill.cpp:283-355; nombre = col 2).
//! - `parse_interface` — `uiscriptlocale.py` `LoadLocaleFile` (líneas
//!   `key\tvalue`; el valor es el SEGUNDO token — los tabs extra se ignoran).
//! - `parse_names_dump` — el dump de DumpProto (`VNUM\tLOCALE_NAME`; el
//!   nombre es la col 1, truncada al segundo tab — parity
//!   `AsStringByIndex(1)`).
//! - index/Setting/Town — parity del filtro de `load_map_spawns`
//!   (realm::npc.rs:139-179).
//!
//! Codificación (verificada contra los bytes reales, 2026-08-12):
//! - El pack se exporta en windows-1252 (`alsoExportToCharset`) salvo ae
//!   (árabe, 1256) y ru (cirílico, 1251) — los dumps lo confirman.
//! - Las CLAVES de locale_string_XX.txt son coreano EUC-KR/CP949 (el lexer
//!   del servidor está modificado para EUC-KR) → `decode_key`.
//! - Los VALORES se decodifican por idioma → `decode_lang` (UTF-8 estricto
//!   primero: un archivo ya UTF-8 pasa intacto).

use encoding_rs::{Encoding, EUC_KR, WINDOWS_1251, WINDOWS_1252, WINDOWS_1256};

/// Codepage del idioma (parity del export del pack: windows-1252 salvo ae
/// (árabe, 1256) y ru (cirílico, 1251) — bytes reales verificados 2026-08-12).
fn lang_encoding(lang: &str) -> &'static Encoding {
    match lang {
        "ae" => WINDOWS_1256,
        "ru" => WINDOWS_1251,
        _ => WINDOWS_1252,
    }
}

/// Decodifica texto de un campo por idioma: UTF-8 estricto primero; si no,
/// el codepage del idioma (ver doc del módulo). Almacenar UTF-8 en PG.
pub fn decode_lang(bytes: &[u8], lang: &str) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => lang_encoding(lang).decode(bytes).0.into_owned(),
    }
}

/// Decodifica una CLAVE de locale_string: UTF-8 estricto primero; si no,
/// EUC-KR/CP949 (las claves base del runtime son coreano — parity del lexer
/// del servidor, llex.c modificado para EUC-KR, AGENTS.md §15).
pub fn decode_key(bytes: &[u8]) -> String {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => EUC_KR.decode(bytes).0.into_owned(),
    }
}

/// Divide el buffer en líneas (parity `CMemoryTextFileLoader::Bind`,
/// FileLoader.cpp:144-178): saltos en `\n` o `\r`; un par CRLF/LFCR cuenta
/// como UN salto; el contenido conserva los bytes crudos (los caracteres
/// con el bit alto, >= 0x80, viajan byte a byte — los saltos son ASCII y
/// nunca aparecen dentro de un par EUC-KR).
fn split_lines(buf: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < buf.len() {
        if buf[i] == b'\n' || buf[i] == b'\r' {
            lines.push(&buf[start..i]);
            let next = buf.get(i + 1).copied();
            if matches!((buf[i], next), (b'\n', Some(b'\r')) | (b'\r', Some(b'\n'))) {
                i += 1;
            }
            start = i + 1;
        }
        i += 1;
    }
    lines.push(&buf[start..]);
    lines
}

/// Fin de la cita que empieza en `start` (parity `quote_find_end`,
/// locale.cpp:132-159): el `;` fuera de comillas termina; `\"` dentro de
/// comillas NO alterna el estado (se salta la pareja).
fn quote_find_end(b: &[u8], start: usize) -> Option<usize> {
    let mut quote = false;
    let mut i = start;
    while i < b.len() {
        if quote && b[i] == b'\\' && i + 1 < b.len() {
            if b[i + 1] == b'"' {
                i += 2;
                continue;
            }
        } else if b[i] == b'"' {
            quote = !quote;
        } else if !quote && b[i] == b';' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Convierte el segmento entre comillas (parity `locale_convert`,
/// locale.cpp:161-217): `\n` (backslash-n) → salto real; `\"` → `"`;
/// cualquier otro carácter se copia tal cual; `;` fuera de comillas corta.
fn locale_convert(src: &[u8]) -> Option<Vec<u8>> {
    let len = src.len();
    let mut out = Vec::with_capacity(len);
    let mut start = false;
    let mut last = 0u8;
    let mut i = 0;
    while i < len {
        let c = src[i];
        let mut encode = false;
        if c == b'"' {
            if last != b'\\' {
                start = !start;
            } else {
                encode = true;
            }
        } else if c == b';' {
            if last != b'\\' && !start {
                break;
            } else {
                encode = true;
            }
        } else if start {
            encode = true;
        }
        if encode {
            if c == b'\\' && i + 1 < len && src[i + 1] == b'n' {
                out.push(b'\n');
                i += 1;
                last = b'\n';
            } else {
                out.push(c);
                last = c;
            }
        }
        i += 1;
    }
    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

/// Parser de locale_string_XX.txt (parity `locale_init_file`, locale.cpp:
/// 222-307). Devuelve los pares (clave, valor) en bytes YA convertidos
/// (escapes aplicados) pero SIN decodificar — el importador elige la
/// codepage (clave: `decode_key`; valor: `decode_lang`).
///
/// Semántica replicada:
/// - Entradas `"clave";` + whitespace + `"valor";` (el `;` tras la cierre).
/// - Un par malformado (sin cierre, sin valor, formato inválido) DETIENE el
///   parseo del resto del archivo (parity `break`).
/// - Bytes NUL interiores cortan el archivo (parity `while (tmp && *tmp)`).
pub fn parse_locale_string(buf: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let n = buf.len();
    let mut tmp = 0;
    let mut out = Vec::new();
    while tmp < n && buf[tmp] != 0 {
        if buf[tmp] == b'"' {
            let mut strings: [Option<Vec<u8>>; 2] = [None, None];
            for (i, slot) in strings.iter_mut().enumerate() {
                let Some(end) = quote_find_end(buf, tmp) else { break };
                *slot = locale_convert(&buf[tmp..end]);
                tmp = end + 1;
                while tmp < n && matches!(buf[tmp], b'\n' | b'\r' | b' ') {
                    tmp += 1;
                }
                if i + 1 == 2 {
                    break;
                }
                if tmp >= n || buf[tmp] != b'"' {
                    break; // parity: invalid format -> strings[1] sigue null
                }
            }
            let (Some(key), Some(value)) = (&strings[0], &strings[1]) else {
                break; // parity: strings[0] == nullptr || strings[1] == nullptr
            };
            out.push((key.clone(), value.clone()));
        } else {
            match buf[tmp..].iter().position(|&c| c == b'\n') {
                Some(off) => tmp += off + 1,
                None => break, // parity: strchr -> nullptr -> fin
            }
        }
    }
    out
}

/// Dump de nombres de DumpProto (`VNUM\tLOCALE_NAME`): la primera línea es
/// el header (se omite — parity `nameData.Next()`). Vnum = col 0; nombre =
/// col 1, truncado al segundo tab (parity `AsStringByIndex(1)`). Líneas sin
/// tab se ignoran (parity `ColCount() >= 2`). Decodifica por idioma.
pub fn parse_names_dump(bytes: &[u8], lang: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for (i, line) in split_lines(bytes).iter().enumerate() {
        if i == 0 {
            continue; // header "VNUM\tLOCALE_NAME"
        }
        let Some(tab1) = line.iter().position(|&c| c == b'\t') else {
            continue; // ColCount < 2
        };
        let Ok(vnum) = std::str::from_utf8(&line[..tab1]).unwrap_or("").trim().parse::<i64>()
        else {
            continue;
        };
        let name_end = line[tab1 + 1..]
            .iter()
            .position(|&c| c == b'\t')
            .map(|p| tab1 + 1 + p)
            .unwrap_or(line.len());
        out.push((vnum, decode_lang(&line[tab1 + 1..name_end], lang)));
    }
    out
}

/// `__SnapString` parity (ItemManager.cpp:178-195): si empieza con `"`, quita
/// el primer carácter y, si termina con `"`, también el último.
fn snap_string(s: &[u8]) -> Vec<u8> {
    if s.len() < 2 || s[0] != b'"' {
        return s.to_vec();
    }
    let end = if s[s.len() - 1] == b'"' { s.len() - 1 } else { s.len() };
    s[1..end].to_vec()
}

/// itemdesc.txt (parity `LoadItemDesc`, ItemManager.cpp:197-236): líneas con
/// tabs; col 0 = vnum, col 1 = desc, col 2 = summ; líneas vacías ignoradas
/// (SplitLineByTab); columnas ausentes rellenadas con ""; el desc pasa por
/// `__SnapString`. DESVIACIÓN documentada: los desc vacíos NO se importan
/// (el C++ los carga para no romper el tooltip; una fila vacía no aporta
/// información a `common.item_descriptions`).
pub fn parse_itemdesc(bytes: &[u8], lang: &str) -> Vec<(i64, String)> {
    let mut out = Vec::new();
    for line in split_lines(bytes) {
        if line.is_empty() {
            continue; // SplitLineByTab -> false
        }
        let cols: Vec<&[u8]> = line.split(|&c| c == b'\t').collect();
        let Ok(vnum) = std::str::from_utf8(cols[0]).unwrap_or("").trim().parse::<i64>() else {
            continue;
        };
        let desc = snap_string(cols.get(1).copied().unwrap_or(b""));
        if !desc.is_empty() {
            out.push((vnum, decode_lang(&desc, lang)));
        }
    }
    out
}

/// skilldesc.txt (parity `RegisterSkillDesc`, PythonSkill.cpp:283-355):
/// col 0 = skill_id (0 -> omitido, parity `NO_INDEX_ERROR`), col 1 = clase,
/// col 2 = nombre (`DESC_TOKEN_TYPE_NAME1`). Líneas con < 3 columnas se
/// omiten (guarda defensiva: el C++ indexa col 2 sin verificar — los
/// archivos reales tienen ~28 columnas).
pub fn parse_skilldesc(bytes: &[u8], lang: &str) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    for line in split_lines(bytes) {
        if line.is_empty() {
            continue;
        }
        let cols: Vec<&[u8]> = line.split(|&c| c == b'\t').collect();
        if cols.len() < 3 {
            continue; // DESC_TOKEN_TYPE_NAME1 fuera de rango (defensivo)
        }
        let Ok(id) = std::str::from_utf8(cols[0]).unwrap_or("").trim().parse::<i32>() else {
            continue;
        };
        if id == 0 {
            continue; // parity iSkillIndex == 0
        }
        out.push((id, decode_lang(cols[2], lang)));
    }
    out
}

/// locale_interface.txt (parity `LoadLocaleFile`, uiscriptlocale.py:25-32):
/// `key\tvalue`; el valor es el SEGUNDO token (`tokens[1]` — los tabs extra
/// se ignoran); líneas sin tab se descartan (`len(tokens) < 2`); una línea
/// `key\t` produce valor vacío. La línea se parte por el primer tab y el
/// valor llega hasta el segundo tab o fin de línea.
pub fn parse_interface(bytes: &[u8], lang: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in split_lines(bytes) {
        let Some(tab1) = line.iter().position(|&c| c == b'\t') else {
            continue;
        };
        let val_start = tab1 + 1;
        let val_end = line[val_start..]
            .iter()
            .position(|&c| c == b'\t')
            .map(|p| val_start + p)
            .unwrap_or(line.len());
        out.push((
            decode_lang(&line[..tab1], lang),
            decode_lang(&line[val_start..val_end], lang),
        ));
    }
    out
}

/// index del runtime de mapas (`id nombre` por línea — parity del filtro de
/// `load_map_spawns`, realm::npc.rs:139-151: líneas vacías y comentarios
/// `#`/`/` ignorados; el nombre es el segundo token whitespace).
pub fn parse_map_index(bytes: &[u8]) -> Vec<(i32, String)> {
    let mut out = Vec::new();
    for line in split_lines(bytes) {
        let t = String::from_utf8_lossy(line);
        let t = t.trim();
        if t.is_empty() || t.starts_with(['/', '#']) {
            continue;
        }
        let mut it = t.split_whitespace();
        if let (Some(id), Some(name)) = (it.next(), it.next())
            && let Ok(id) = id.parse::<i32>()
        {
            out.push((id, name.to_string()));
        }
    }
    out
}

/// BasePosition del Setting.txt (parity `load_map_spawns`,
/// realm::npc.rs:154-167): la línea cuyo primer token (sin distinguir
/// mayúsculas) es `BasePosition`; los dos siguientes son x/y en UNITS.
pub fn parse_setting_base(bytes: &[u8]) -> Option<(i32, i32)> {
    for line in split_lines(bytes) {
        let line = String::from_utf8_lossy(line);
        let mut it = line.split_whitespace();
        if let (Some(k), Some(x), Some(y)) = (it.next(), it.next(), it.next())
            && k.eq_ignore_ascii_case("BasePosition")
        {
            return Some((x.parse().ok()?, y.parse().ok()?));
        }
    }
    None
}

/// Town.txt: los DOS primeros enteros whitespace (parity `LoadMapRegion` /
/// `load_map_spawns`, realm::npc.rs:171-179) — el posSpawn del mapa en UNITS
/// = base + (x*100, y*100). Archivo ausente, vacío o con no-enteros → el
/// propio base (parity del `unwrap_or(base)` del npc.rs).
pub fn parse_town_spawn(town_bytes: Option<&[u8]>, base: (i32, i32)) -> (i32, i32) {
    let Some(bytes) = town_bytes else { return base };
    let text = String::from_utf8_lossy(bytes);
    let mut it = text.split_whitespace();
    let (Some(x), Some(y)) = (it.next(), it.next()) else { return base };
    match (x.parse::<i32>(), y.parse::<i32>()) {
        (Ok(x), Ok(y)) => (base.0 + x * 100, base.1 + y * 100),
        _ => base,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- locale_string (parity locale.cpp) ----

    /// Formato REAL del runtime: `"key";\r\n"value";\r\n` con líneas en
    /// blanco entre entradas y claves coreano CP949 (bytes crudos).
    #[test]
    fn locale_string_real_format() {
        let text = b"\"The name...\";\r\n\"The name...\";\r\n\r\n\"%s \xBA\xED\xB7\xB0! (%d%%)\";\r\n\"%s blocked! (%d%%)\";\r\n";
        let pairs = parse_locale_string(text);
        assert_eq!(pairs.len(), 2, "{pairs:?}");
        assert_eq!(pairs[0].0, b"The name...");
        assert_eq!(pairs[0].1, b"The name...");
        assert_eq!(pairs[1].0, b"%s \xBA\xED\xB7\xB0! (%d%%)");
        assert_eq!(pairs[1].1, b"%s blocked! (%d%%)");
    }

    /// `\"` dentro de comillas NO cierra la cita (quote_find_end salta la
    /// pareja) pero el backslash SE CONSERVA en la salida (locale_convert
    /// solo desescapa `\n` — parity exacta); `;` dentro de comillas se
    /// conserva; el `;` tras la cierre termina la entrada.
    #[test]
    fn locale_string_escapes_and_semicolon() {
        let text = b"\"say \\\"hi\\\"\";\r\n\"line1\\nline2; ok\";\r\n";
        let pairs = parse_locale_string(text);
        assert_eq!(pairs.len(), 1, "{pairs:?}");
        // `\"` -> backslash + comilla (parity locale_convert: solo \n se desescapa).
        assert_eq!(pairs[0].0, b"say \\\"hi\\\"");
        assert_eq!(pairs[0].1, b"line1\nline2; ok");
    }

    /// Una entrada malformada DETIENE el parseo (parity break): sin cierre
    /// de cita o sin valor, el resto del archivo se descarta.
    #[test]
    fn locale_string_malformed_stops() {
        // Clave sin cierre (EOF) -> 0 pares.
        let text = b"\"only key";
        assert!(parse_locale_string(text).is_empty(), "sin cierre -> stop");
        // Clave seguida de EOF (sin valor) -> 0 pares.
        let text = b"\"only key\";";
        assert!(parse_locale_string(text).is_empty(), "sin valor -> stop");
        // Válida + malformada -> solo la válida (la malformada corta el resto).
        let text2 = b"\"k1\";\r\n\"v1\";\r\n\"k2\" junk\r\n\"v2\";\r\n";
        let pairs = parse_locale_string(text2);
        assert_eq!(pairs, vec![(b"k1".to_vec(), b"v1".to_vec())]);
        // Clave sin comillas -> la línea se salta; el par siguiente queda
        // incompleto (clave sola) -> 0 pares.
        let text3 = b"k0\r\n\"v0\";\r\n";
        assert!(parse_locale_string(text3).is_empty(), "clave sin comillas -> sin valor");
    }

    /// LF-only también funciona (el runtime actual es CRLF; los dumps son
    /// mixtos — el parser acepta ambos, parity Bind).
    #[test]
    fn locale_string_lf_only() {
        let text = b"\"a\";\n\"b\";\n\n\"c\";\n\"d\";\n";
        let pairs = parse_locale_string(text);
        assert_eq!(pairs, vec![(b"a".to_vec(), b"b".to_vec()), (b"c".to_vec(), b"d".to_vec())]);
    }

    // ---- locale_interface (parity uiscriptlocale.py) ----

    /// `key\tvalue`; valor = segundo token (los tabs extra se ignoran);
    /// `key\t` -> valor vacío; línea sin tab -> descartada.
    #[test]
    fn interface_tab_pairs() {
        let text = b"ACCEPT\tAceptar\nA\tB\tC\nD\t\n\nE\n";
        let pairs = parse_interface(text, "es");
        assert_eq!(
            pairs,
            vec![
                ("ACCEPT".to_string(), "Aceptar".to_string()),
                ("A".to_string(), "B".to_string()),
                ("D".to_string(), String::new()),
            ]
        );
    }

    // ---- itemdesc / skilldesc (parity SplitLineByTab) ----

    /// Col 0 = vnum, col 1 = desc (con __SnapString), vacías ignoradas,
    /// desc vacío omitido (desviación documentada), codepage por idioma.
    #[test]
    fn itemdesc_rows() {
        let text = b"300\tHoja de zod\xEDaco+0\t\n301\t\"Entre comillas\"\n\n302\n303\t\tresumen\n";
        let rows = parse_itemdesc(text, "es");
        assert_eq!(
            rows,
            vec![
                (300, "Hoja de zodíaco+0".to_string()),
                (301, "Entre comillas".to_string()),
            ]
        );
    }

    /// Nombre = col 2 (NAME1); skill_id 0 omitido; línea corta omitida.
    #[test]
    fn skilldesc_name_column() {
        let text = b"1\tWARRIOR\tCorte de tres maneras\tCorte triple\t...\tDesc\n0\tJOB\tX\t...\n106\tSHAMAN\tTiro rel\xE1mpago\t...\n";
        let rows = parse_skilldesc(text, "es");
        assert_eq!(
            rows,
            vec![(1, "Corte de tres maneras".to_string()), (106, "Tiro relámpago".to_string())]
        );
    }

    // ---- dumps de DumpProto ----

    /// Header omitido; vnum/nombre por tab; el nombre se trunca al segundo
    /// tab (parity col 1); líneas sin tab ignoradas; CRLF y LF mixtos.
    #[test]
    fn names_dump_rows() {
        let text = b"VNUM\tLOCALE_NAME\r\n101\tPerro Salvaje\r\n108\tJabal\xED\n102\tLobo\tEXTRA\r\n\nbad\n";
        let rows = parse_names_dump(text, "es");
        assert_eq!(
            rows,
            vec![
                (101, "Perro Salvaje".to_string()),
                (108, "Jabalí".to_string()),
                (102, "Lobo".to_string()),
            ]
        );
    }

    // ---- mapas (parity load_map_spawns) ----

    #[test]
    fn map_index_parity() {
        let text = b"# comentario\n/ otro\n\n1 metin2_map_a1\n41 metin2_map_c1\n\n43 metin2_map_c3 extra\n";
        let idx = parse_map_index(text);
        assert_eq!(
            idx,
            vec![
                (1, "metin2_map_a1".to_string()),
                (41, "metin2_map_c1".to_string()),
                (43, "metin2_map_c3".to_string()),
            ]
        );
    }

    #[test]
    fn setting_and_town() {
        let setting = b"ScriptType\tMapSetting\n\nCellScale\t200\nBasePosition\t921600\t204800\n";
        let base = parse_setting_base(setting).expect("base");
        assert_eq!(base, (921600, 204800));
        let town = b"480 736\n125 1113\n";
        assert_eq!(parse_town_spawn(Some(town), base), (969600, 278400));
        // Town ausente -> el propio base (parity unwrap_or(base)).
        assert_eq!(parse_town_spawn(None, base), base);
        // Setting sin BasePosition -> None.
        assert!(parse_setting_base(b"ScriptType\tMapSetting\n").is_none());
        // Town con no-enteros -> base.
        assert_eq!(parse_town_spawn(Some(b"abc def\n"), base), base);
    }

    /// split_lines: CRLF, LF, CR suelto y mezclas (parity Bind).
    #[test]
    fn split_lines_crlf_and_lf() {
        let text = b"a\r\nb\nc\rd\re";
        let lines = split_lines(text);
        assert_eq!(lines, vec![&b"a"[..], &b"b"[..], &b"c"[..], &b"d"[..], &b"e"[..]]);
    }

    // ---- codificación ----

    /// Clave coreano CP949 real del runtime (BA ED B7 B0): decodifica y el
    /// round-trip EUC-KR devuelve los MISMOS bytes (codepage estable).
    #[test]
    fn decode_key_real_runtime_bytes() {
        let b = [0xBAu8, 0xED, 0xB7, 0xB0];
        let s = decode_key(&b);
        let (re, _, _) = EUC_KR.encode(&s);
        assert_eq!(&re[..], &b[..], "round-trip CP949 estable: {s:?}");
    }

    #[test]
    fn decode_key_utf8_ascii_passthrough() {
        assert_eq!(decode_key(b"The name has been changed."), "The name has been changed.");
        assert_eq!(decode_key("Café ✓".as_bytes()), "Café ✓");
    }

    /// CP1252: el byte 0xED del dump ES es "í" (Jabalí).
    #[test]
    fn decode_lang_cp1252() {
        assert_eq!(decode_lang(b"Jabal\xED", "es"), "Jabalí");
        assert_eq!(decode_lang(b"Tiro rel\xE1mpago", "es"), "Tiro relámpago");
    }

    /// ae -> windows-1256 (round-trip con encoding_rs).
    #[test]
    fn decode_lang_ae_1256() {
        let (bytes, _, _) = WINDOWS_1256.encode("مرحبا");
        assert_eq!(decode_lang(&bytes, "ae"), "مرحبا");
    }

    /// ru -> windows-1251 (round-trip).
    #[test]
    fn decode_lang_ru_1251() {
        let (bytes, _, _) = WINDOWS_1251.encode("Дикий пёс");
        assert_eq!(decode_lang(&bytes, "ru"), "Дикий пёс");
    }

    /// Un archivo ya UTF-8 pasa intacto (cualquier idioma).
    #[test]
    fn decode_lang_utf8_passthrough() {
        let s = "Ünïcode ✓ – déjà vu";
        assert_eq!(decode_lang(s.as_bytes(), "es"), s);
        assert_eq!(decode_lang(s.as_bytes(), "ae"), s);
    }
}
