//! F1 — wire del locale: `CG_LOCALE_REQUEST` (132) / `GC_LOCALE` (140).
//!
//! Spec: `docs/plans/locale-redesign.md` §Wire + ADR-0009. Aditivo (patrón
//! `datachannel`): el cliente legacy registra los headers como no-op — el
//! wire del login no cambia.
//!
//! Wire:
//! - `CG_LOCALE_REQUEST` (client → auth): 4 B = header + `lang[3]` ASCII
//!   ("es\0" — 2 letras + NUL, parity del `szLanguage` del LOGIN3 auth).
//! - `GC_LOCALE` (auth → client): chunked variable-length. Cada paquete wire:
//!   `0x8c` + `u16 payload_len` (longitud de TODO lo que sigue al campo) +
//!   `u8 chunk_flag` (1 = hay más chunks, 0 = final) + bytes del chunk. El
//!   cliente reensambla los chunks en orden de llegada; con flag 0 el buffer
//!   está completo.
//! - Buffer completo (payload reensamblado): `u8 section_count` + por sección
//!   `u8 kind` + `u32 count` + `count × (u16 key_len + key + u16 val_len +
//!   val)`. Claves ASCII ("101", "173217", "INVENTORY"). kinds 0..=5
//!   (mob/item/item_desc/skill/map/ui — message_texts e item_icons NO se
//!   envían, ADR-0009).
//!
//! Disciplina del crate: LE manual, sin panics ante entrada malformada —
//! `encode_*` es total (trunca defensivamente), `decode_*` devuelve
//! `Result<_, LocaleError>`.

use crate::header;
use crate::{from_cstr, ProtocolError, Result};

/// `CG_LOCALE_REQUEST` (client → auth): header + lang[3].
pub const HEADER_CG_LOCALE_REQUEST: u8 = header::CG_LOCALE_REQUEST;
/// `GC_LOCALE` (auth → client): chunked variable-length.
pub const HEADER_GC_LOCALE: u8 = header::GC_LOCALE;

/// Kinds de sección del `GC_LOCALE` (0..=5; el byte viaja tal cual).
pub const SECTION_MOB: u8 = 0;
pub const SECTION_ITEM: u8 = 1;
pub const SECTION_ITEM_DESC: u8 = 2;
pub const SECTION_SKILL: u8 = 3;
pub const SECTION_MAP: u8 = 4;
pub const SECTION_UI: u8 = 5;
/// Secciones del wire actual (el buffer empieza con `section_count` = 6).
pub const SECTION_COUNT: usize = 6;

/// Error de parseo del `GC_LOCALE`. Nunca panic: entrada malformada → `Err`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocaleError {
    /// Faltan bytes para completar la estructura declarada (o un chunk con
    /// longitud incoherente / sin flag final).
    Truncated,
    /// `section_count` != 6 (el wire actual fija las 6 secciones).
    SectionCount(u8),
    /// kind de sección fuera de 0..=5.
    UnknownSection(u8),
}

/// El bundle completo del locale: 6 secciones de pares (clave, valor).
/// Claves ASCII (los ids numéricos casteados a texto); valores UTF-8.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocaleBundle {
    pub mob: Vec<(String, String)>,
    pub item: Vec<(String, String)>,
    pub item_desc: Vec<(String, String)>,
    pub skill: Vec<(String, String)>,
    pub map: Vec<(String, String)>,
    pub ui: Vec<(String, String)>,
}

impl LocaleBundle {
    /// Total de pares del bundle (suma de las 6 secciones).
    pub fn len(&self) -> usize {
        self.mob.len()
            + self.item.len()
            + self.item_desc.len()
            + self.skill.len()
            + self.map.len()
            + self.ui.len()
    }

    /// `true` si no hay ningún par en ninguna sección.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// `CG_LOCALE_REQUEST` (4 B): `BYTE header` + `BYTE lang[3]` ("es\0").
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct CgLocaleRequest {
    pub header: u8,
    pub lang: [u8; 3],
}

impl CgLocaleRequest {
    pub const SIZE: usize = 4;
    pub const HEADER: u8 = HEADER_CG_LOCALE_REQUEST;

    /// `lang` se trunca/NUL-pad a 3 bytes (`from_cstr` — "es" → "es\0").
    pub fn new(lang: &str) -> Self {
        Self { header: Self::HEADER, lang: from_cstr(lang) }
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        if data.len() != Self::SIZE {
            return Err(ProtocolError::BadLength { expected: Self::SIZE, got: data.len() });
        }
        Ok(Self { header: data[0], lang: [data[1], data[2], data[3]] })
    }

    pub fn to_bytes(&self) -> [u8; Self::SIZE] {
        [self.header, self.lang[0], self.lang[1], self.lang[2]]
    }
}

/// Serializa el buffer completo del `GC_LOCALE` (SIN el sobre chunked):
/// `u8 section_count` + por sección `u8 kind` + `u32 count` + pares
/// `(u16 key_len, key, u16 val_len, val)`. LE. Defensivo: claves/valores
/// mayores a `u16::MAX` se truncan (nunca ocurre en los datos reales —
/// claves cortas, valores < 4 KB).
pub fn encode_payload(bundle: &LocaleBundle) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(SECTION_COUNT as u8);
    for (kind, rows) in [
        (SECTION_MOB, &bundle.mob),
        (SECTION_ITEM, &bundle.item),
        (SECTION_ITEM_DESC, &bundle.item_desc),
        (SECTION_SKILL, &bundle.skill),
        (SECTION_MAP, &bundle.map),
        (SECTION_UI, &bundle.ui),
    ] {
        out.push(kind);
        out.extend_from_slice(&(rows.len() as u32).to_le_bytes());
        for (k, v) in rows {
            let k = &k.as_bytes()[..k.len().min(u16::MAX as usize)];
            let v = &v.as_bytes()[..v.len().min(u16::MAX as usize)];
            out.extend_from_slice(&(k.len() as u16).to_le_bytes());
            out.extend_from_slice(k);
            out.extend_from_slice(&(v.len() as u16).to_le_bytes());
            out.extend_from_slice(v);
        }
    }
    out
}

/// Parsea el buffer completo del `GC_LOCALE` (SIN el sobre chunked).
/// Entrada malformada → `LocaleError` (nunca panic). `section_count` != 6 →
/// `SectionCount`; kind fuera de 0..=5 → `UnknownSection` (el parseo del
/// cliente se detiene — el wire actual no tiene más secciones).
pub fn decode_payload(payload: &[u8]) -> std::result::Result<LocaleBundle, LocaleError> {
    let mut bundle = LocaleBundle::default();
    let mut pos = 0usize;
    // Lee `n` bytes o `Truncated` (el cierre del slice evita panics).
    let take = |pos: &mut usize, n: usize| -> std::result::Result<&[u8], LocaleError> {
        let end = pos.checked_add(n).filter(|&e| e <= payload.len()).ok_or(LocaleError::Truncated)?;
        let s = &payload[*pos..end];
        *pos = end;
        Ok(s)
    };

    let section_count = take(&mut pos, 1)?[0];
    if section_count != SECTION_COUNT as u8 {
        return Err(LocaleError::SectionCount(section_count));
    }
    for _ in 0..section_count {
        let kind = take(&mut pos, 1)?[0];
        let count = u32::from_le_bytes(take(&mut pos, 4)?.try_into().expect("4 bytes"));
        for _ in 0..count {
            let key_len = u16::from_le_bytes(take(&mut pos, 2)?.try_into().expect("2 bytes")) as usize;
            let key = take(&mut pos, key_len)?;
            let val_len = u16::from_le_bytes(take(&mut pos, 2)?.try_into().expect("2 bytes")) as usize;
            let val = take(&mut pos, val_len)?;
            let pair = (
                String::from_utf8_lossy(key).into_owned(),
                String::from_utf8_lossy(val).into_owned(),
            );
            match kind {
                SECTION_MOB => bundle.mob.push(pair),
                SECTION_ITEM => bundle.item.push(pair),
                SECTION_ITEM_DESC => bundle.item_desc.push(pair),
                SECTION_SKILL => bundle.skill.push(pair),
                SECTION_MAP => bundle.map.push(pair),
                SECTION_UI => bundle.ui.push(pair),
                other => return Err(LocaleError::UnknownSection(other)),
            }
        }
    }
    Ok(bundle)
}

/// Parte el payload en paquetes wire del `GC_LOCALE`: `0x8c` + `u16
/// payload_len` (todo lo que sigue al campo: flag + chunk) + `u8 chunk_flag`
/// (1 = hay más, 0 = final) + chunk. `max_chunk` = bytes de chunk por
/// paquete (el auth usa 64_000 — el wire queda en 64_004 B); se acota a
/// 65_534 para que `payload_len = 1 + len` quepa en u16. El último chunk
/// lleva flag 0; un payload vacío emite UN chunk final vacío (el cliente
/// completa el buffer con `section_count` = 0).
pub fn encode_chunks(payload: &[u8], max_chunk: usize) -> Vec<Vec<u8>> {
    let max_chunk = max_chunk.clamp(1, u16::MAX as usize - 1);
    let mut out = Vec::new();
    let mut pos = 0;
    loop {
        let end = (pos + max_chunk).min(payload.len());
        let more = end < payload.len();
        let mut pkt = Vec::with_capacity(4 + (end - pos));
        pkt.push(HEADER_GC_LOCALE);
        pkt.extend_from_slice(&((1 + (end - pos)) as u16).to_le_bytes());
        pkt.push(if more { 1 } else { 0 });
        pkt.extend_from_slice(&payload[pos..end]);
        out.push(pkt);
        pos = end;
        if !more {
            break;
        }
    }
    out
}

/// Reensambla los chunks wire del `GC_LOCALE` (en orden de llegada) y
/// devuelve el payload completo. Sin flag final (0) antes del fin → o con
/// longitud incoherente → `Truncated`.
pub fn decode_chunks(chunks: &[&[u8]]) -> std::result::Result<Vec<u8>, LocaleError> {
    let mut payload = Vec::new();
    for (i, c) in chunks.iter().enumerate() {
        if c.len() < 4 || c[0] != HEADER_GC_LOCALE {
            return Err(LocaleError::Truncated);
        }
        let len = u16::from_le_bytes([c[1], c[2]]) as usize;
        if c.len() != 3 + len {
            return Err(LocaleError::Truncated);
        }
        let flag = c[3];
        payload.extend_from_slice(&c[4..]);
        if flag == 0 {
            // El final debe ser el ÚLTIMO chunk (chunks extra = malformado).
            if i + 1 != chunks.len() {
                return Err(LocaleError::Truncated);
            }
            return Ok(payload);
        }
    }
    Err(LocaleError::Truncated) // nunca llegó el flag final
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bundle fixture con las 6 secciones (claves/vals realistas).
    fn fixture() -> LocaleBundle {
        LocaleBundle {
            mob: vec![("101".into(), "Perro Salvaje".into()), ("2101".into(), "Zorro del Desierto".into())],
            item: vec![("1".into(), "Yang".into()), ("10".into(), "Espada +0".into())],
            item_desc: vec![("300".into(), "Hoja de zodíaco+0".into())],
            skill: vec![("1".into(), "Corte de tres maneras".into())],
            map: vec![("41".into(), "metin2_map_c1".into())],
            ui: vec![("ACCEPT".into(), "Aceptar".into()), ("ATTACH_METIN_INFO".into(), "¿Quieres usar la Piedra Espíritu?".into())],
        }
    }

    /// El wire del request: header 132 + "es\0".
    #[test]
    fn request_wire_exact() {
        let req = CgLocaleRequest::new("es");
        assert_eq!(req.to_bytes(), [132, b'e', b's', 0]);
        assert_eq!(CgLocaleRequest::SIZE, 4);
        // Truncamiento defensivo a 3 bytes ("esp" → "es" + NUL).
        assert_eq!(CgLocaleRequest::new("esp").to_bytes(), [132, b'e', b's', 0]);
    }

    #[test]
    fn request_roundtrip_and_bad_length() {
        let req = CgLocaleRequest::new("de");
        let parsed = CgLocaleRequest::from_bytes(&req.to_bytes()).expect("parse");
        assert_eq!(parsed, req);
        assert_eq!(parsed.lang, *b"de\0");
        assert!(matches!(
            CgLocaleRequest::from_bytes(&[132, b'e']),
            Err(ProtocolError::BadLength { expected: 4, got: 2 })
        ));
        assert!(matches!(
            CgLocaleRequest::from_bytes(&[132, b'e', b's', 0, 0]),
            Err(ProtocolError::BadLength { expected: 4, got: 5 })
        ));
    }

    /// Roundtrip encode → decode: bundle idéntico.
    #[test]
    fn payload_roundtrip() {
        let b = fixture();
        let decoded = decode_payload(&encode_payload(&b)).expect("decode");
        assert_eq!(decoded, b);
    }

    /// Bundle vacío: section_count 6, todas las secciones con count 0.
    #[test]
    fn payload_empty_bundle() {
        let b = LocaleBundle::default();
        let bytes = encode_payload(&b);
        assert_eq!(bytes.len(), 1 + 6 * 5, "6 secciones con count 0 (5 B c/u)");
        assert_eq!(decode_payload(&bytes).expect("decode"), b);
        assert!(b.is_empty());
    }

    /// El wire del payload: bytes exactos de la primera sección.
    #[test]
    fn payload_wire_exact() {
        let b = fixture();
        let bytes = encode_payload(&b);
        assert_eq!(bytes[0], 6, "section_count");
        assert_eq!(bytes[1], SECTION_MOB, "kind 0");
        assert_eq!(
            u32::from_le_bytes([bytes[2], bytes[3], bytes[4], bytes[5]]),
            2,
            "count mob"
        );
        // Primera entrada: key_len 3 + "101" + val_len 13 + "Perro Salvaje".
        assert_eq!(u16::from_le_bytes([bytes[6], bytes[7]]), 3);
        assert_eq!(&bytes[8..11], b"101");
        assert_eq!(u16::from_le_bytes([bytes[11], bytes[12]]), 13);
        assert_eq!(&bytes[13..26], b"Perro Salvaje");
    }

    /// Chunking: cada paquete wire respeta header + u16 len + flag; el
    /// reensamblado es idéntico al payload original.
    #[test]
    fn chunks_roundtrip_and_wire() {
        let payload = encode_payload(&fixture());
        // Chunk pequeño (fuerza varios chunks), mediano y mayor que el payload.
        for max_chunk in [1usize, 7, 32, 64_000] {
            let chunks = encode_chunks(&payload, max_chunk);
            assert!(!chunks.is_empty(), "max_chunk {max_chunk}: al menos un chunk");
            for (i, c) in chunks.iter().enumerate() {
                assert_eq!(c[0], HEADER_GC_LOCALE, "header 0x8c");
                let len = u16::from_le_bytes([c[1], c[2]]) as usize;
                assert_eq!(c.len(), 3 + len, "u16 payload_len = todo lo que sigue");
                let flag = c[3];
                assert!(flag <= 1, "flag 0/1");
                assert_eq!(c.len() - 4, max_chunk.min(payload.len() - i * max_chunk), "tamaño del chunk {i}");
                assert_eq!(c[3], if i + 1 < chunks.len() { 1 } else { 0 }, "flag del chunk {i}");
            }
            let joined: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
            assert_eq!(decode_chunks(&joined).expect("reensamblar"), payload, "max_chunk {max_chunk}");
        }
    }

    /// Payload vacío → un solo chunk final (flag 0, sin bytes).
    #[test]
    fn chunks_empty_payload() {
        let chunks = encode_chunks(&[], 64_000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], [HEADER_GC_LOCALE, 1, 0, 0], "header + len 1 + flag 0");
        let joined: Vec<&[u8]> = chunks.iter().map(|c| c.as_slice()).collect();
        assert_eq!(decode_chunks(&joined).expect("reensamblar"), Vec::<u8>::new());
    }

    /// El chunk de tamaño máximo: 64_000 bytes de chunk → wire de 64_004 B
    /// (el límite del spec F1); `max_chunk` excesivo se acota a u16::MAX - 1
    /// (payload_len = 1 + len no desborda) y el resto va en otro chunk.
    #[test]
    fn chunks_max_size_wire() {
        let payload = vec![0xABu8; 64_000];
        let chunks = encode_chunks(&payload, 64_000);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].len(), 64_004, "wire = 1 + 2 + 1 + 64000");
        assert_eq!(chunks[0][0], HEADER_GC_LOCALE);
        assert_eq!(u16::from_le_bytes([chunks[0][1], chunks[0][2]]), 64_001);
        // max_chunk excesivo (100_000) se acota a 65_534.
        let big_payload = vec![0xABu8; 100_000];
        let big = encode_chunks(&big_payload, 100_000);
        assert_eq!(big.len(), 2, "65_534 + 34_466");
        assert_eq!(big[0].len(), 1 + 2 + 1 + 65_534, "capado a u16::MAX - 1");
        assert_eq!(u16::from_le_bytes([big[0][1], big[0][2]]), u16::MAX, "payload_len máximo");
        assert_eq!(big[0][3], 1, "hay más chunks");
        assert_eq!(big[1][3], 0, "el último es final");
        let joined: Vec<&[u8]> = big.iter().map(|c| c.as_slice()).collect();
        assert_eq!(decode_chunks(&joined).expect("reensamblar"), big_payload);
    }

    /// Entrada malformada → Err, NUNCA panic.
    #[test]
    fn decode_malformed_no_panics() {
        // Payload truncado en cada posición (hasta 40 B).
        let payload = encode_payload(&fixture());
        for cut in 0..payload.len().min(40) {
            let r = decode_payload(&payload[..cut]);
            assert!(r.is_err(), "truncado en {cut} debe fallar");
        }
        // section_count inválido.
        let mut bad = payload.clone();
        bad[0] = 5;
        assert_eq!(decode_payload(&bad), Err(LocaleError::SectionCount(5)));
        let mut bad = payload.clone();
        bad[0] = 7;
        assert_eq!(decode_payload(&bad), Err(LocaleError::SectionCount(7)));
        // kind inválido (0x06) en la primera sección.
        let mut bad = payload.clone();
        bad[1] = 6;
        assert_eq!(decode_payload(&bad), Err(LocaleError::UnknownSection(6)));
        // key_len mayor que el resto → Truncated.
        let mut bad = payload.clone();
        bad[6] = 0xFF;
        bad[7] = 0xFF;
        assert_eq!(decode_payload(&bad), Err(LocaleError::Truncated));
        // Chunks: longitud incoherente, header malo, sin flag final, flag
        // final seguido de otro chunk → Truncated.
        assert_eq!(decode_chunks(&[]), Err(LocaleError::Truncated));
        assert_eq!(decode_chunks(&[&[0x8c, 5, 0, 0]]), Err(LocaleError::Truncated), "len != resto");
        assert_eq!(decode_chunks(&[&[0x00, 1, 0, 0]]), Err(LocaleError::Truncated), "header malo");
        assert_eq!(decode_chunks(&[&[0x8c, 1, 0, 1]]), Err(LocaleError::Truncated), "sin flag final");
        let final_chunk = [0x8cu8, 1, 0, 0];
        let extra = [0x8cu8, 1, 0, 1];
        assert_eq!(decode_chunks(&[&final_chunk, &extra]), Err(LocaleError::Truncated), "chunk tras el final");
    }

    /// Valores no-UTF-8 en el payload se decodifican lossy (nunca panic).
    /// Un `String` del bundle siempre es UTF-8 válido — el payload se
    /// construye a mano con las 6 secciones (solo UI con 1 entrada) y un
    /// byte suelto (0xE9) como valor.
    #[test]
    fn decode_non_utf8_values_lossy() {
        let mut payload = vec![SECTION_COUNT as u8];
        for kind in 0..SECTION_COUNT as u8 {
            payload.push(kind);
            if kind == SECTION_UI {
                payload.extend_from_slice(&1u32.to_le_bytes()); // count 1
                payload.extend_from_slice(&1u16.to_le_bytes());
                payload.push(b'K');
                payload.extend_from_slice(&1u16.to_le_bytes());
                payload.push(0xE9); // byte suelto — UTF-8 inválido
            } else {
                payload.extend_from_slice(&0u32.to_le_bytes()); // count 0
            }
        }
        let decoded = decode_payload(&payload).expect("decode");
        assert_eq!(decoded.ui, vec![("K".to_string(), "\u{FFFD}".to_string())], "lossy, sin panic");
    }
}
