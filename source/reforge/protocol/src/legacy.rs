//! `protocol::legacy` — paquetes legacy-client-only (ADR-0006): aislados en un
//! boundary borrable en bloque en F7 (cuando el cliente legacy desaparezca).
//!
//! Contenido: los paquetes que el auth C++ envía en login exitoso ANTES del
//! `GC_AUTH_SUCCESS` (`input_db.cpp:1710-1716`):
//!
//! - **151 `GC_PANAMA_PACK`** (289 B): `BYTE header; char szPackName[256];
//!   BYTE abIV[32]` — el IV de cada pack se XOR-ea por DWORD con
//!   `panamaKey + i*16777619` (parity `panama.cpp:70-93` envío /
//!   `EterPack.cpp:276-287` descifrado del cliente).
//! - **152 `GC_HYBRIDCRYPT_KEYS`**: `header + u16 size + i32 key_stream_len +
//!   stream` (parity `desc_manager.cpp:527-542`, `GetPackageCryptKeys`).
//! - **153 `GC_HYBRIDCRYPT_SDB`**: `header + u16 size + i32 stream_len +
//!   stream` (parity `desc_manager.cpp:544-565`, `GetRelatedMapSDBStreams`).
//!
//! Los payloads de 151-153 se cargan de archivos del runtime del server
//! (`panama/panama.lst` + archivos IV; `cshybridcrypt*`) — parity
//! `panama.cpp:8-58` y `ClientPackageCryptInfo.cpp`. El runtime actual de
//! `srv1` NO tiene esos archivos → el auth C++ NO envía estos paquetes hoy;
//! la carga aquí replica la lógica para cuando existan.

use std::collections::HashMap;
use std::path::Path;

pub const GC_PANAMA_PACK: u8 = 151;
pub const GC_HYBRIDCRYPT_KEYS: u8 = 152;
pub const GC_HYBRIDCRYPT_SDB: u8 = 153;

// ---------------------------------------------------------------------------
// 151 — PanamaPack
// ---------------------------------------------------------------------------

/// 151 — `TPacketGCPanamaPack` (289 B): header + szPackName[256] + abIV[32]
/// con el IV XOR-eado por DWORD (`ivs[i] ^= panama_key + i*16777619`,
/// parity `panama.cpp:88-92`).
pub struct PanamaPack;

impl PanamaPack {
    pub const SIZE: usize = 1 + 256 + 32;

    /// Serializa un paquete 151. `iv` es el IV crudo del archivo; el XOR con
    /// `panama_key` se aplica aquí (el mismo algoritmo del descifrado cliente).
    pub fn encode(pack_name: &str, iv: [u8; 32], panama_key: u32) -> [u8; Self::SIZE] {
        let mut b = [0u8; Self::SIZE];
        b[0] = GC_PANAMA_PACK;
        let name = pack_name.as_bytes();
        let n = name.len().min(255);
        b[1..1 + n].copy_from_slice(&name[..n]);
        let mut iv = iv;
        for (i, chunk) in iv.chunks_exact_mut(4).enumerate() {
            let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let xored = v ^ panama_key.wrapping_add(i as u32 * 16_777_619);
            chunk.copy_from_slice(&xored.to_le_bytes());
        }
        b[257..289].copy_from_slice(&iv);
        b
    }
}

// ---------------------------------------------------------------------------
// 152/153 — hybrid-crypt
// ---------------------------------------------------------------------------

/// 152 — `TPacketGCHybridCryptKeys`: header + u16 tamaño dinámico + i32
/// `KeyStreamLen` + stream (parity `desc_manager.cpp:527-542`).
pub struct HybridCryptKeys(pub Vec<u8>);

impl HybridCryptKeys {
    pub fn new(stream: Vec<u8>) -> Self {
        Self(stream)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size = 7 + self.0.len();
        let mut b = Vec::with_capacity(size);
        b.push(GC_HYBRIDCRYPT_KEYS);
        b.extend((size as u16).to_le_bytes());
        b.extend((self.0.len() as i32).to_le_bytes());
        b.extend_from_slice(&self.0);
        b
    }
}

/// 153 — `TPacketGCPackageSDB`: header + u16 tamaño dinámico + i32
/// `iStreamLen` + stream (parity `desc_manager.cpp:544-565`).
pub struct PackageSDB(pub Vec<u8>);

impl PackageSDB {
    pub fn new(stream: Vec<u8>) -> Self {
        Self(stream)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let size = 7 + self.0.len();
        let mut b = Vec::with_capacity(size);
        b.push(GC_HYBRIDCRYPT_SDB);
        b.extend((size as u16).to_le_bytes());
        b.extend((self.0.len() as i32).to_le_bytes());
        b.extend_from_slice(&self.0);
        b
    }
}

// ---------------------------------------------------------------------------
// Carga de archivos del runtime (parity panama.cpp / ClientPackageCryptInfo.cpp)
// ---------------------------------------------------------------------------

/// Entrada de `panama/panama.lst`: nombre del pack + IV de 32 bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PanamaEntry {
    pub name: String,
    pub iv: [u8; 32],
}

/// Datos hybrid-crypt cargados (vacíos = no enviar — parity del C++: los
/// loops/checks de `KeyStreamLen > 0` / `iStreamLen > 0` no envían nada).
#[derive(Debug, Clone, Default)]
pub struct HybridData {
    /// Stream serializado de keys: `i32 package_count` + por archivo
    /// `i32 key_size` + bytes (parity `GetPackageCryptKeys`).
    pub keys_stream: Vec<u8>,
    /// Streams SDB serializados por mapa (clave lowercase; `MAPNAME_DEFAULT`
    /// = "none", `input_db.cpp:46`).
    pub sdb: HashMap<String, Vec<u8>>,
}

/// Carga `panama/panama.lst` + archivos IV (parity `PanamaLoad`,
/// `panama.cpp:8-58`): cada línea `packname ivfile`.
pub fn load_panama(dir: &Path) -> Vec<PanamaEntry> {
    let mut out = Vec::new();
    let Ok(list) = std::fs::read_to_string(dir.join("panama.lst")) else {
        return out;
    };
    for line in list.lines() {
        let mut fields = line.split_whitespace();
        let (Some(name), Some(iv_file)) = (fields.next(), fields.next()) else {
            continue;
        };
        let Ok(iv_bytes) = std::fs::read(dir.join(iv_file)) else {
            continue;
        };
        if iv_bytes.len() != 32 {
            continue;
        }
        let mut iv = [0u8; 32];
        iv.copy_from_slice(&iv_bytes);
        out.push(PanamaEntry {
            name: name.to_string(),
            iv,
        });
    }
    out
}

/// Carga los archivos `cshybridcrypt*` del dir (parity
/// `LoadPackageCryptInfo`/`LoadPackageCryptFile`): keys + bloque SDB.
pub fn load_hybrid(dir: &Path) -> HybridData {
    let mut keys: Vec<u8> = Vec::new();
    let mut package_count: i32 = 0;
    let mut sdb: HashMap<String, Vec<u8>> = HashMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return HybridData::default();
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.contains("cshybridcrypt") {
            continue;
        }
        let Ok(data) = std::fs::read(entry.path()) else {
            continue;
        };
        parse_hybrid_file(&data, &mut keys, &mut package_count, &mut sdb);
    }
    let mut keys_stream = Vec::new();
    if !keys.is_empty() {
        keys_stream.extend(package_count.to_le_bytes());
        keys_stream.extend(&keys);
    }
    HybridData { keys_stream, sdb }
}

/// Parseo de un archivo hybrid (parity `LoadPackageCryptFile`):
/// `i32 iSDBDataOffset, i32 iPackageCnt, [keys], bloque SDB`.
/// El stream SDB por mapa replica el "stream to client" del comentario del
/// C++: `i32 count` + por entrada `u32 file hash, u32 map name size, map name,
/// u8 block size, blocks`.
fn parse_hybrid_file(
    data: &[u8],
    keys: &mut Vec<u8>,
    package_count: &mut i32,
    sdb: &mut HashMap<String, Vec<u8>>,
) {
    if data.len() < 8 {
        return;
    }
    let offset = i32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
    *package_count += i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    if offset <= 8 || offset > data.len() {
        return;
    }
    // keys: `i32 key_size` + bytes (parity LoadPackageCryptFile).
    let key_size = offset - 8;
    keys.extend((key_size as i32).to_le_bytes());
    keys.extend_from_slice(&data[8..offset]);

    // Bloque SDB.
    let mut pos = offset;
    let sdb_cnt = rd_i32(data, &mut pos).unwrap_or(0) as usize;
    for _ in 0..sdb_cnt {
        let _pkg_hash = rd_u32(data, &mut pos);
        let _stream_size = rd_u32(data, &mut pos);
        let file_cnt = rd_u32(data, &mut pos).unwrap_or(0) as usize;
        for _ in 0..file_cnt {
            let file_hash = rd_u32(data, &mut pos).unwrap_or(0);
            let map_size = rd_u32(data, &mut pos).unwrap_or(0) as usize;
            let Some(map_bytes) = take(data, &mut pos, map_size) else {
                return;
            };
            let map_name = String::from_utf8_lossy(map_bytes)
                .to_string()
                .to_lowercase();
            let block_size = take(data, &mut pos, 1).map(|b| b[0] as usize).unwrap_or(0);
            let Some(blocks) = take(data, &mut pos, block_size) else {
                return;
            };
            // stream serializado por mapa (parity GetSerializedStream):
            // i32 count + por entrada: file hash, map name, block size, blocks.
            let stream = sdb.entry(map_name).or_default();
            if stream.is_empty() {
                stream.extend(1i32.to_le_bytes());
            } else {
                // múltiples archivos del mismo mapa → count += 1 (reescribe el
                // count: formato simple — count al inicio)
                let count = i32::from_le_bytes([stream[0], stream[1], stream[2], stream[3]]) + 1;
                stream[..4].copy_from_slice(&count.to_le_bytes());
            }
            stream.extend(file_hash.to_le_bytes());
            stream.extend((map_size as u32).to_le_bytes());
            stream.extend(map_bytes);
            stream.push(block_size as u8);
            stream.extend_from_slice(blocks);
        }
    }
}

fn rd_i32(data: &[u8], pos: &mut usize) -> Option<i32> {
    let b = take(data, pos, 4)?;
    Some(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn rd_u32(data: &[u8], pos: &mut usize) -> Option<u32> {
    let b = take(data, pos, 4)?;
    Some(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn take<'a>(data: &'a [u8], pos: &mut usize, n: usize) -> Option<&'a [u8]> {
    if *pos + n > data.len() {
        return None;
    }
    let s = &data[*pos..*pos + n];
    *pos += n;
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 151: 289 B, header, nombre, y XOR del IV recuperable (parity
    /// EterPack.cpp:276-287 — el cliente aplica el mismo XOR para descifrar).
    #[test]
    fn panama_pack_size_and_xor() {
        let key = 0x1234_5678u32;
        let iv = [0u8; 32]; // IV crudo de ceros
        let pkt = PanamaPack::encode("test.epk", iv, key);
        assert_eq!(pkt.len(), 289);
        assert_eq!(pkt[0], GC_PANAMA_PACK);
        assert_eq!(&pkt[1..9], b"test.epk");
        // el primer DWORD del IV = key ^ 0 (iv crudo cero)
        assert_eq!(
            u32::from_le_bytes([pkt[257], pkt[258], pkt[259], pkt[260]]),
            key
        );
        // el segundo DWORD = key + 1*16777619
        assert_eq!(
            u32::from_le_bytes([pkt[261], pkt[262], pkt[263], pkt[264]]),
            key.wrapping_add(16_777_619)
        );
        // el cliente "descifra" con el mismo XOR y recupera el IV crudo
        let mut ivs = [0u8; 32];
        ivs.copy_from_slice(&pkt[257..289]);
        for (i, chunk) in ivs.chunks_exact_mut(4).enumerate() {
            let v = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            let x = v ^ key.wrapping_add(i as u32 * 16_777_619);
            chunk.copy_from_slice(&x.to_le_bytes());
        }
        assert_eq!(ivs, iv);
    }

    /// 152/153: wire exacto — header + u16 size + i32 len + stream.
    #[test]
    fn hybrid_wire_format() {
        let keys = HybridCryptKeys::new(vec![1, 2, 3]).to_bytes();
        assert_eq!(keys, vec![152, 10, 0, 3, 0, 0, 0, 1, 2, 3]); // size 7+3=10
        let sdb = PackageSDB::new(vec![0xde, 0xad]).to_bytes();
        assert_eq!(sdb, vec![153, 9, 0, 2, 0, 0, 0, 0xde, 0xad]);
    }

    /// Carga de panama.lst + archivos IV (sintético).
    #[test]
    fn load_panama_from_dir() {
        let dir = std::env::temp_dir().join("f2a_legacy_panama_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("panama.lst"), "test.epk iv1.bin\n").unwrap();
        std::fs::write(dir.join("iv1.bin"), [0xAAu8; 32]).unwrap();
        let entries = load_panama(&dir);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "test.epk");
        assert_eq!(entries[0].iv, [0xAAu8; 32]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Carga hybrid sintética: keys + SDB del mapa "none".
    #[test]
    fn load_hybrid_from_dir() {
        let dir = std::env::temp_dir().join("f2a_legacy_hybrid_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // archivo: [i32 offset][i32 cnt][keys (offset-8 B)][i32 sdb_cnt][pkg: hash, stream_size,
        //          file_cnt, file: hash, map_size, "none", block_size, blocks]
        let mut f = Vec::new();
        let key = vec![0x11, 0x22, 0x33];
        let offset = 8 + key.len(); // el offset apunta al bloque SDB
        f.extend((offset as i32).to_le_bytes());
        f.extend(1i32.to_le_bytes()); // package count
        f.extend_from_slice(&key);
        f.extend(1i32.to_le_bytes()); // sdb package cnt
        f.extend(77u32.to_le_bytes()); // pkg name hash
        f.extend(10u32.to_le_bytes()); // stream size
        f.extend(1u32.to_le_bytes()); // file cnt
        f.extend(0xDEAD_BEEFu32.to_le_bytes()); // file hash
        f.extend(4u32.to_le_bytes()); // map name size
        f.extend(b"none");
        f.push(2); // block size
        f.extend([0xAA, 0xBB]);
        std::fs::write(dir.join("cshybridcrypt1.bin"), &f).unwrap();

        let hybrid = load_hybrid(&dir);
        assert_eq!(
            hybrid.keys_stream.len(),
            4 + 4 + 3,
            "i32 cnt + i32 size + keys"
        );
        let sdb = hybrid.sdb.get("none").expect("sdb del mapa none");
        // stream: i32 count=1 + file hash + map size + "none" + block size + blocks
        assert_eq!(&sdb[..4], &1i32.to_le_bytes());
        assert_eq!(&sdb[4..8], &0xDEAD_BEEFu32.to_le_bytes());
        assert_eq!(&sdb[8..12], &4u32.to_le_bytes());
        assert_eq!(&sdb[12..16], b"none");
        assert_eq!(sdb[16], 2);
        assert_eq!(&sdb[17..19], &[0xAA, 0xBB]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Sin archivos → vacío (parity: el runtime actual no envía 151-153).
    #[test]
    fn load_empty_when_no_files() {
        let dir = std::env::temp_dir().join("f2a_legacy_empty_test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(load_panama(&dir).is_empty());
        assert!(load_hybrid(&dir).keys_stream.is_empty());
        assert!(load_hybrid(&dir).sdb.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
