//! F5.4 (ADR-0011): walkability server-side — port de `IsMovablePosition`.
//!
//! Parity `sectree_manager.cpp` (leído completo — evidencia con línea):
//!
//! - **Formato del archivo de atributos** (`LoadAttribute`, `:372-470`): el
//!   server lee `{map_path}/{map_name}/server_attr` — cabecera `i32 tiles_w,
//!   i32 tiles_h` + por tile (bucle `for y … for x` — row-major en el archivo):
//!   `i32 uiSize` + bloque **LZO1X** que descomprime a
//!   `sizeof(DWORD) * (SECTREE_SIZE/CELL_SIZE)² = 65536 B` (`:387,447`) —
//!   una grid de celdas `DWORD` (bitmask de atributos), 128×128 por tile.
//!   Verificado empíricamente en el runtime desplegado (mapa 41
//!   `metin2_map_c1/server_attr`: cabecera 16×20 tiles — `MapSize 4 5` ×
//!   `CellScale 200` → `iWidth = CellScale*128*4 = 102400 u` = 16 tiles de
//!   6400 u — primer tile 272 B comprimidos → 65536 B).
//! - **Constantes** (`sectree.h:8-10,27-30`): `SECTREE_SIZE = 6400`,
//!   `CELL_SIZE = 50` → 128 celdas por tile (la celda es de **50 units**, no
//!   100); `ATTR_BLOCK = 1<<0`, `ATTR_OBJECT = 1<<7`.
//! - **`IsMovablePosition`** (`:753-761`): movible iff
//!   `!(attr & (ATTR_BLOCK | ATTR_OBJECT))`. Fuera del mapa (sectree null)
//!   → **no movible**.
//! - **Indexación de celda** (`sectree.cpp:208`): tree = `(x/6400, y/6400)`
//!   (`sectree_manager.cpp:75-76`), tile en el archivo =
//!   `x/6400 - base_x/6400` (`:403-404`), celda dentro del tile =
//!   `(x % 6400) / 50` — MISMA aritmética entera truncante que el C++ (los
//!   `BasePosition` reales son múltiplos de 6400; la división truncante de
//!   Rust coincide con la del C++ en estos rangos).
//! - **`Setting.txt`** (`LoadSettingFile`, `:163-208`): `MapSize w h`
//!   (tiles de 128×128 celdas), `BasePosition x y` (units),
//!   `CellScale` → `iWidth = CellScale * 128 * w` (units).
//! - **`index`** (`Build`, `:654-741`): `mapId mapName` por línea; comentarios
//!   `#`/`/`; el path del mapa = `{map_path}/{mapName}/Setting.txt` (`:690`),
//!   atributos = `{map_path}/{mapName}/server_attr` (`:718`).
//!
//! El C++ **NO valida walkability en `CInputMain::Move`** (`input_main.cpp:
//! 1437-1599` — solo timer speedhack activo; el check de distancia está
//! comentado). Este módulo es la lectura del `server_attr` que el canal usa
//! como control de ADR-0011: la walkability es un refuerzo del anti-teleport
//! (un salto fuera del envelope no puede aterrizar en terreno bloqueado; los
//! pasos normales se aceptan como el C++ — el cliente valida su propia
//! colisión, píxel a píxel). Diagnóstico 2026-08-13: la FUENTE del pueblo
//! (celdas locales 62-63 del tile 7,11) es ATTR_BLOCK **real** del archivo —
//! el parse era correcto; el gate previo del canal atascaba al jugador en su
//! borde (el cliente permite posiciones dentro de la celda que el modelo no
//! ocupa). Fallo de carga del mapa → el canal loguea y OMITE el chequeo
//! (fail-open; el envelope anti-speedhack sigue activo) — un mapa roto no
//! congela a los jugadores.

use std::collections::HashMap;
use std::fmt;
use std::path::Path;
use std::sync::Mutex;

/// Lado de un sectree en units (`sectree.h:8`).
pub const SECTREE_SIZE: i32 = 6400;
/// Lado de una celda de atributo en units (`sectree.h:10`).
pub const CELL_SIZE: i32 = 50;
/// Celdas por lado de tile = SECTREE_SIZE / CELL_SIZE = 128 (`sectree.h` +
/// `sectree_manager.cpp:460`).
pub const TILE_CELLS: usize = 128;
/// Bytes descomprimidos por tile = 128×128×4 (`sectree_manager.cpp:447`).
pub const TILE_BYTES: usize = TILE_CELLS * TILE_CELLS * 4;

/// `ATTR_BLOCK = 1 << 0` (`sectree.h:27`).
pub const ATTR_BLOCK: u32 = 1 << 0;
/// `ATTR_OBJECT = 1 << 7` (`sectree.h:30`).
pub const ATTR_OBJECT: u32 = 1 << 7;
/// Máscara de no-movible de `IsMovablePosition` (`sectree_manager.cpp:760`).
pub const ATTR_MOVABLE_MASK: u32 = ATTR_BLOCK | ATTR_OBJECT;

/// Error de carga de un mapa (los fallos se cachean en el `MapStore` para no
/// re-leer el disco por cada MOVE).
#[derive(Debug)]
pub enum MapLoadError {
    /// I/O de un archivo del mapa.
    Io {
        map: i32,
        path: String,
        source: std::io::Error,
    },
    /// El `index` no se pudo leer/parsear (problema global del map_path).
    Index(String),
    /// `Setting.txt` inválido para el mapa.
    Setting { map: i32, source: String },
    /// `server_attr` inválido/ilegible para el mapa.
    Attr { map: i32, source: String },
    /// Fallo de carga previo (cacheado — el canal no re-intenta por MOVE).
    Cached { map: i32, source: String },
}

impl fmt::Display for MapLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapLoadError::Io { map, path, source } => {
                write!(f, "mapa {map}: no se pudo leer {path}: {source}")
            }
            MapLoadError::Index(e) => write!(f, "index de mapas: {e}"),
            MapLoadError::Setting { map, source } => write!(f, "mapa {map}: Setting.txt: {source}"),
            MapLoadError::Attr { map, source } => write!(f, "mapa {map}: server_attr: {source}"),
            MapLoadError::Cached { map, source } => {
                write!(f, "mapa {map}: carga fallida antes: {source}")
            }
        }
    }
}

impl std::error::Error for MapLoadError {}

/// Setting del mapa parseado de `Setting.txt` — parity `LoadSettingFile`
/// (`sectree_manager.cpp:163-208`): `iWidth = CellScale * 128 * width` units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapSetting {
    pub index: i32,
    pub name: String,
    pub base_x: i32,
    pub base_y: i32,
    /// Ancho del mapa en units (= CellScale × 128 × tiles_w).
    pub width: i32,
    /// Alto del mapa en units (= CellScale × 128 × tiles_h).
    pub height: i32,
}

/// Grid de atributos de un mapa: celdas de 50 units, row-major `[y][x]`,
/// `tiles_w*128 × tiles_h*128` — la misma disposición que el C++ monta en
/// `SECTREE_MAP` (`LoadAttribute` + `CAttribute::Get` = `dwordPtr[y][x]`,
/// `attribute.cc:216-231`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapData {
    pub setting: MapSetting,
    tiles_w: usize,
    tiles_h: usize,
    cells: Vec<u32>,
}

impl MapData {
    /// Atributo de la celda en UNITS (parity `SECTREE::IsAttr`,
    /// `sectree.cpp:208`). `None` = fuera del mapa (parity: `Get()` devuelve
    /// null tree → `IsMovablePosition` false).
    pub fn attr(&self, x: i32, y: i32) -> Option<u32> {
        if x < self.setting.base_x || y < self.setting.base_y {
            return None;
        }
        if x >= self.setting.base_x + self.setting.width
            || y >= self.setting.base_y + self.setting.height
        {
            return None;
        }
        // Tree del punto: x/6400 (sectree_manager.cpp:75-76); tile en el
        // archivo: x/6400 - base_x/6400 (LoadAttribute :403-404); celda
        // dentro del tile: (x % 6400) / 50 (sectree.cpp:208) — aritmética
        // truncante idéntica al C++ (base_x múltiplo de 6400 en los mapas).
        let tx = (x / SECTREE_SIZE) - (self.setting.base_x / SECTREE_SIZE);
        let ty = (y / SECTREE_SIZE) - (self.setting.base_y / SECTREE_SIZE);
        let cx = (x % SECTREE_SIZE) / CELL_SIZE;
        let cy = (y % SECTREE_SIZE) / CELL_SIZE;
        let (Ok(tx), Ok(ty), Ok(cx), Ok(cy)) = (
            usize::try_from(tx),
            usize::try_from(ty),
            usize::try_from(cx),
            usize::try_from(cy),
        ) else {
            return None;
        };
        if tx >= self.tiles_w || ty >= self.tiles_h {
            return None;
        }
        let cols = self.tiles_w * TILE_CELLS;
        let row = ty * TILE_CELLS + cy;
        let col = tx * TILE_CELLS + cx;
        self.cells.get(row * cols + col).copied()
    }

    /// `IsMovablePosition` (sectree_manager.cpp:753-761):
    /// `!(attr & (ATTR_BLOCK | ATTR_OBJECT))`. Fuera del mapa → NO movible
    /// (parity: `Get()` → null tree → `return false`).
    pub fn is_movable(&self, x: i32, y: i32) -> bool {
        match self.attr(x, y) {
            Some(a) => a & ATTR_MOVABLE_MASK == 0,
            None => false,
        }
    }

    /// Primera celda MOVIBLE del mapa (fallback del `GetValidLocation` —
    /// P0-B 2026-08-14): barrido row-major de la grid (parity del layout
    /// `[y][x]` row-major de `attr`). Devuelve el CENTRO de la celda en
    /// UNITS. Cualquier mapa con terreno tiene al menos una celda movible.
    pub fn first_movable(&self) -> Option<(i32, i32)> {
        let cols = self.tiles_w * TILE_CELLS;
        for (i, cell) in self.cells.iter().enumerate() {
            if cell & ATTR_MOVABLE_MASK == 0 {
                let row = i / cols;
                let col = i % cols;
                let x = self.setting.base_x + (col as i32) * CELL_SIZE + CELL_SIZE / 2;
                let y = self.setting.base_y + (row as i32) * CELL_SIZE + CELL_SIZE / 2;
                return Some((x, y));
            }
        }
        None
    }
}

/// Parse de `Setting.txt` (parity `LoadSettingFile`, sectree_manager.cpp:163-208).
pub fn parse_setting(index: i32, name: &str, content: &str) -> Result<MapSetting, String> {
    let (mut width, mut height, mut base_x, mut base_y, mut cell_scale) =
        (0i32, 0i32, 0i32, 0i32, 0i32);
    for line in content.lines() {
        let mut tokens = line.split_whitespace();
        let Some(cmd) = tokens.next() else { continue };
        match cmd.to_ascii_lowercase().as_str() {
            "mapsize" => {
                let (Some(Ok(w)), Some(Ok(h))) = (
                    tokens.next().map(|t| t.parse()),
                    tokens.next().map(|t| t.parse()),
                ) else {
                    return Err(format!("MapSize inválido: '{line}'"));
                };
                (width, height) = (w, h);
            }
            "baseposition" => {
                let (Some(Ok(x)), Some(Ok(y))) = (
                    tokens.next().map(|t| t.parse()),
                    tokens.next().map(|t| t.parse()),
                ) else {
                    return Err(format!("BasePosition inválido: '{line}'"));
                };
                (base_x, base_y) = (x, y);
            }
            "cellscale" => {
                let Some(Ok(scale)) = tokens.next().map(|t| t.parse()) else {
                    return Err(format!("CellScale inválido: '{line}'"));
                };
                cell_scale = scale;
            }
            _ => {} // ScriptType/HeightScale/ViewRadius/TextureSet/Environment — ignorados.
        }
    }
    if (width == 0 && height == 0) || cell_scale == 0 {
        return Err(format!(
            "dims inválidas (MapSize {width}x{height}, CellScale {cell_scale}) — parity :198-202"
        ));
    }
    Ok(MapSetting {
        index,
        name: name.to_string(),
        base_x,
        base_y,
        width: cell_scale * 128 * width,
        height: cell_scale * 128 * height,
    })
}

/// Lee el `index` del map_path (parity `Build`, sectree_manager.cpp:654-688):
/// líneas `mapId mapName`; se saltan vacías y comentarios `#`/`/`.
pub fn read_index(map_path: &str) -> Result<HashMap<i32, String>, MapLoadError> {
    let path = Path::new(map_path).join("index");
    let content = std::fs::read_to_string(&path).map_err(|e| MapLoadError::Io {
        map: -1,
        path: path.display().to_string(),
        source: e,
    })?;
    let mut index = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with('/') {
            continue;
        }
        let mut tokens = line.split_whitespace();
        let (Some(id), Some(name)) = (tokens.next(), tokens.next()) else {
            return Err(MapLoadError::Index(format!("línea inválida: '{line}'")));
        };
        let Ok(id) = id.parse::<i32>() else {
            return Err(MapLoadError::Index(format!("mapId no numérico: '{id}'")));
        };
        index.insert(id, name.to_string());
    }
    Ok(index)
}

/// Grid de atributos cruda del `server_attr` (parity `LoadAttribute`,
/// sectree_manager.cpp:372-470): cabecera `i32 tiles_w, i32 tiles_h` + por
/// tile (row-major en archivo) `i32 uiSize` + bloque LZO1X → 65536 B exactos
/// (128×128 u32 LE). Devuelve `(tiles_w, tiles_h, cells)` row-major global.
pub fn parse_attr_grid(bytes: &[u8], map: i32) -> Result<(usize, usize, Vec<u32>), MapLoadError> {
    fn rd_i32(bytes: &[u8], at: &mut usize) -> Option<i32> {
        let v = bytes.get(*at..*at + 4)?;
        *at += 4;
        Some(i32::from_le_bytes([v[0], v[1], v[2], v[3]]))
    }
    let mut at = 0;
    let tiles_w = rd_i32(bytes, &mut at).ok_or_else(|| MapLoadError::Attr {
        map,
        source: "cabecera truncada (tiles_w)".into(),
    })?;
    let tiles_h = rd_i32(bytes, &mut at).ok_or_else(|| MapLoadError::Attr {
        map,
        source: "cabecera truncada (tiles_h)".into(),
    })?;
    if tiles_w <= 0 || tiles_h <= 0 || tiles_w > 4096 || tiles_h > 4096 {
        return Err(MapLoadError::Attr {
            map,
            source: format!("dims de tiles inválidas: {tiles_w}x{tiles_h}"),
        });
    }
    let (tw, th) = (tiles_w as usize, tiles_h as usize);
    let cols = tw * TILE_CELLS;
    let rows = th * TILE_CELLS;
    let mut cells = vec![0u32; cols * rows];
    for ty in 0..th {
        for tx in 0..tw {
            let ui_size = rd_i32(bytes, &mut at).ok_or_else(|| MapLoadError::Attr {
                map,
                source: format!("tile ({tx},{ty}): tamaño comprimido truncado"),
            })?;
            if ui_size < 0 || ui_size as usize > bytes.len() - at {
                return Err(MapLoadError::Attr {
                    map,
                    source: format!(
                        "tile ({tx},{ty}): bloque comprimido fuera de rango ({ui_size} B)"
                    ),
                });
            }
            let comp = &bytes[at..at + ui_size as usize];
            at += ui_size as usize;
            let mut out = [0u8; TILE_BYTES];
            let n = lzo::decompress_into(comp, &mut out).map_err(|e| MapLoadError::Attr {
                map,
                source: format!("tile ({tx},{ty}): lzo: {e}"),
            })?;
            if n != TILE_BYTES {
                return Err(MapLoadError::Attr {
                    map,
                    source: format!("tile ({tx},{ty}): {n} B descomprimidos != {TILE_BYTES}"),
                });
            }
            // Tile → grid global: fila global = ty*128 + cy, col = tx*128 + cx.
            for cy in 0..TILE_CELLS {
                let src_row = cy * TILE_CELLS * 4;
                let dst_row = (ty * TILE_CELLS + cy) * cols + tx * TILE_CELLS;
                for cx in 0..TILE_CELLS {
                    let off = src_row + cx * 4;
                    cells[dst_row + cx] =
                        u32::from_le_bytes([out[off], out[off + 1], out[off + 2], out[off + 3]]);
                }
            }
        }
    }
    Ok((tw, th, cells))
}

/// Carga un mapa completo desde el map_path del server (parity `Build` +
/// `LoadAttribute`): `index` → `Setting.txt` + `server_attr`.
pub fn load_map(map_path: &str, index: i32, name: &str) -> Result<MapData, MapLoadError> {
    let dir = Path::new(map_path).join(name);
    let setting_path = dir.join("Setting.txt");
    let content = std::fs::read_to_string(&setting_path).map_err(|e| MapLoadError::Io {
        map: index,
        path: setting_path.display().to_string(),
        source: e,
    })?;
    let setting = parse_setting(index, name, &content).map_err(|e| MapLoadError::Setting {
        map: index,
        source: e,
    })?;
    let attr_path = dir.join("server_attr");
    let bytes = std::fs::read(&attr_path).map_err(|e| MapLoadError::Io {
        map: index,
        path: attr_path.display().to_string(),
        source: e,
    })?;
    let (tiles_w, tiles_h, cells) = parse_attr_grid(&bytes, index)?;
    Ok(MapData {
        setting,
        tiles_w,
        tiles_h,
        cells,
    })
}

/// Caché de mapas cargados + fallos (el canal no re-lee disco por MOVE).
#[derive(Default)]
pub struct MapStore {
    index: Option<HashMap<i32, String>>,
    maps: HashMap<i32, MapData>,
    failed: HashMap<i32, String>,
}

impl MapStore {
    pub fn new() -> Self {
        Self::default()
    }

    fn index_of(&mut self, map_path: &str) -> Result<&HashMap<i32, String>, MapLoadError> {
        if self.index.is_none() {
            self.index = Some(read_index(map_path)?);
        }
        Ok(self.index.as_ref().expect("index recién cargado"))
    }

    /// Get-or-load del mapa (los fallos se cachean).
    pub fn load(&mut self, map_path: &str, map_id: i32) -> Result<(), MapLoadError> {
        if self.maps.contains_key(&map_id) {
            return Ok(());
        }
        if let Some(source) = self.failed.get(&map_id) {
            return Err(MapLoadError::Cached {
                map: map_id,
                source: source.clone(),
            });
        }
        // Mapas de instancia (≥ 10000): mismas celdas que su mapa base
        // (parity CreatePrivateMap — el C++ copia el sectree).
        let base = if map_id >= 10000 {
            map_id / 10000
        } else {
            map_id
        };
        let name = match self.index_of(map_path) {
            Ok(idx) => match idx.get(&base).cloned() {
                Some(n) => n,
                None => {
                    let e = MapLoadError::Setting {
                        map: map_id,
                        source: format!("mapa {base} no está en el index"),
                    };
                    self.failed.insert(map_id, e.to_string());
                    return Err(e);
                }
            },
            Err(e) => {
                // El fallo del index también se cachea (el canal no re-intenta
                // por MOVE con un map_path roto).
                self.failed.insert(map_id, e.to_string());
                return Err(e);
            }
        };
        match load_map(map_path, base, &name) {
            Ok(map) => {
                self.maps.insert(map_id, map);
                Ok(())
            }
            Err(e) => {
                self.failed.insert(map_id, e.to_string());
                Err(e)
            }
        }
    }

    pub fn get(&self, map_id: i32) -> Option<&MapData> {
        self.maps.get(&map_id)
    }

    /// `IsMovablePosition` con get-or-load. `Err` = el mapa no cargó (el
    /// canal decide fail-open); `Ok(false)` = destino realmente no movible.
    pub fn is_movable(
        &mut self,
        map_path: &str,
        map_id: i32,
        x: i32,
        y: i32,
    ) -> Result<bool, MapLoadError> {
        self.load(map_path, map_id)?;
        Ok(self.get(map_id).is_some_and(|m| m.is_movable(x, y)))
    }
}

/// Helper del canal: `IsMovablePosition` con la caché compartida entre
/// conexiones (patrón `spawn_cache` del channel).
pub fn is_movable(
    store: &Mutex<MapStore>,
    map_path: &str,
    map_id: i32,
    x: i32,
    y: i32,
) -> Result<bool, MapLoadError> {
    let mut store = store.lock().unwrap_or_else(|p| p.into_inner());
    store.is_movable(map_path, map_id, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map_41_setting() -> MapSetting {
        MapSetting {
            index: 41,
            name: "metin2_map_c1".into(),
            base_x: 921_600,
            base_y: 204_800,
            width: 102_400,
            height: 128_000,
        }
    }

    /// Diagnóstico 2026-08-16: las coords del crash del cliente (987103,
    /// 314720) — ¿son terreno válido del mapa 41?
    #[test]
    fn check_crash_coords_987103_314720() {
        let map_path = "../../deploy/main/srv1/share/locale/spain/map";
        let attr = Path::new(map_path)
            .join("metin2_map_c1")
            .join("server_attr");
        if !attr.exists() {
            eprintln!("SKIP: runtime sin deploy");
            return;
        }
        let bytes = std::fs::read(&attr).expect("server_attr legible");
        let (tw, th, cells) = parse_attr_grid(&bytes, 41).expect("grid");
        let m = MapData {
            setting: map_41_setting(),
            tiles_w: tw,
            tiles_h: th,
            cells,
        };
        eprintln!(
            "coords crash (987103,314720): movible={}",
            m.is_movable(987_103, 314_720)
        );
        eprintln!(
            "spawn pueblo (969600,278400): movible={}",
            m.is_movable(969_600, 278_400)
        );
        eprintln!(
            "fuera del mapa (999999,999999): movible={}",
            m.is_movable(9_999_999, 9_999_999)
        );
    }

    /// Grid sintética 2×1 tiles (256×128 celdas de 50 u), todo movible salvo
    /// las celdas marcadas.
    fn synthetic() -> MapData {
        let (tw, th) = (2usize, 1usize);
        let cols = tw * TILE_CELLS;
        let rows = th * TILE_CELLS;
        let mut cells = vec![0u32; cols * rows];
        // Celda (tx=1, cx=5, cy=7) bloqueada; celda (tx=0, cx=3, cy=4) objeto.
        cells[7 * cols + (TILE_CELLS + 5)] = ATTR_BLOCK;
        cells[4 * cols + 3] = ATTR_OBJECT;
        MapData {
            setting: map_41_setting(),
            tiles_w: tw,
            tiles_h: th,
            cells,
        }
    }

    fn unit_x(cx: usize) -> i32 {
        map_41_setting().base_x + (cx as i32) * CELL_SIZE
    }

    fn unit_y(cy: usize) -> i32 {
        map_41_setting().base_y + (cy as i32) * CELL_SIZE
    }

    /// Parse del Setting.txt REAL del mapa 41 (el runtime desplegado) —
    /// valores verificados contra la boot del C++ (base 921600,204800 —
    /// Town.txt (480,736) → spawn (969600,278400)).
    #[test]
    fn parse_setting_map41() {
        let content = "ScriptType\tMapSetting\n\nCellScale\t200\nHeightScale\t0.500000\n\nViewRadius\t128\n\nMapSize\t4\t5\nBasePosition\t921600\t204800\nTextureSet\tmetin2_C1.txt\n";
        let s = parse_setting(41, "metin2_map_c1", content).expect("Setting válido");
        assert_eq!(s.base_x, 921_600);
        assert_eq!(s.base_y, 204_800);
        assert_eq!(s.width, 102_400, "CellScale*128*4");
        assert_eq!(s.height, 128_000, "CellScale*128*5");
    }

    #[test]
    fn parse_setting_invalid_rejected() {
        assert!(parse_setting(41, "m", "").is_err(), "sin MapSize/CellScale");
        assert!(
            parse_setting(41, "m", "MapSize 0 0\nCellScale 200").is_err(),
            "dims 0 (parity :198)"
        );
    }

    /// `is_movable` sobre la grid sintética: celdas bloqueadas, el resto
    /// movible; fuera del mapa → NO movible (parity `Get()` null).
    #[test]
    fn is_movable_synthetic_grid() {
        let m = synthetic();
        // Default movible (attr = 0).
        assert!(m.is_movable(unit_x(0), unit_y(0)));
        // Celda (tx=1, cx=5, cy=7) → ATTR_BLOCK.
        let bx = unit_x(TILE_CELLS + 5);
        let by = unit_y(7);
        assert_eq!(m.attr(bx, by), Some(ATTR_BLOCK));
        assert!(!m.is_movable(bx, by), "bloqueada");
        // La vecina (tx=1, cx=4) sigue movible — no es un muro de 2 celdas.
        assert!(m.is_movable(unit_x(TILE_CELLS + 4), by));
        // Celda (tx=0, cx=3, cy=4) → ATTR_OBJECT.
        assert!(!m.is_movable(unit_x(3), unit_y(4)), "objeto");
        // Fuera del mapa: izquierda, derecha, abajo.
        assert!(!m.is_movable(m.setting.base_x - 50, m.setting.base_y));
        assert!(!m.is_movable(m.setting.base_x + m.setting.width, m.setting.base_y));
        assert!(!m.is_movable(m.setting.base_x, m.setting.base_y + m.setting.height));
    }

    /// La conversión units→celda del C++ (x/6400, (x%6400)/50) coincide con
    /// la indexación global (base_x múltiplo de 6400) en ambos tiles.
    #[test]
    fn units_to_cell_parity_cpp() {
        let m = synthetic();
        // Tile 0: x = base + cx*50 → celda global cx.
        assert_eq!(m.attr(unit_x(3), unit_y(4)), Some(ATTR_OBJECT));
        // Tile 1: x = base + 6400 + cx*50 → x/6400 = base/6400 + 1,
        // x%6400 = cx*50 → celda global 128+cx.
        assert_eq!(m.attr(unit_x(TILE_CELLS + 5), unit_y(7)), Some(ATTR_BLOCK));
        // El spawn real del mapa 41 (969600, 278400) cae en el tile
        // (7, 11) del grid — consistente con la geometría del Setting.
        let spawn_x = 969_600i32;
        let spawn_y = 278_400i32;
        let tx = spawn_x / SECTREE_SIZE - m.setting.base_x / SECTREE_SIZE;
        let ty = spawn_y / SECTREE_SIZE - m.setting.base_y / SECTREE_SIZE;
        assert_eq!((tx, ty), (7, 11));
        // Celda DENTRO del tile (parity sectree.cpp:208): (x%6400)/50.
        assert_eq!((spawn_x % SECTREE_SIZE) / CELL_SIZE, 64);
        assert_eq!((spawn_y % SECTREE_SIZE) / CELL_SIZE, 64);
        // Celda GLOBAL ((x-base)/50): 7*128+64 = 960 — ambas aritméticas
        // coinciden porque base_x es múltiplo de 6400.
        assert_eq!((spawn_x - m.setting.base_x) / CELL_SIZE, 960);
        assert_eq!((spawn_y - m.setting.base_y) / CELL_SIZE, 1472);
    }

    /// Spot-check con el server_attr REAL del runtime desplegado
    /// (source/deploy — gitignored). Se salta (log) si el runtime no existe:
    /// la suite no depende del deploy.
    #[test]
    fn server_attr_map41_spot_check() {
        let map_path = "../../deploy/main/srv1/share/locale/spain/map";
        let attr = Path::new(map_path)
            .join("metin2_map_c1")
            .join("server_attr");
        if !attr.exists() {
            eprintln!(
                "SKIP server_attr_map41_spot_check: {} no existe (runtime sin deploy)",
                attr.display()
            );
            return;
        }
        let bytes = std::fs::read(&attr).expect("server_attr legible");
        let (tw, th, cells) = parse_attr_grid(&bytes, 41).expect("grid del mapa 41");
        assert_eq!((tw, th), (16, 20), "tiles del Setting (4x5 x CellScale/50)");
        assert_eq!(
            cells.len(),
            16 * TILE_CELLS * 20 * TILE_CELLS,
            "2048x2560 celdas"
        );

        let m = MapData {
            setting: map_41_setting(),
            tiles_w: tw,
            tiles_h: th,
            cells,
        };

        // El spawn del jugador (Town.txt → base + (480,736)*100) ES movible
        // (la plaza del pueblo); el canal lo acepta en el primer MOVE.
        assert!(m.is_movable(969_600, 278_400), "spawn del mapa 41 movible");

        // Sanidad de la descompresión: la grid NO es todo-ceros — hay celdas
        // bloqueadas/objeto (muros del pueblo, edificios) y agua.
        let mut blocked = 0u64;
        let mut water = 0u64;
        for (i, a) in m.cells.iter().enumerate() {
            if a & ATTR_MOVABLE_MASK != 0 {
                blocked += 1;
            }
            if a & (1 << 1) != 0 {
                water += 1;
            }
            if i > 4_000_000 {
                break;
            }
        }
        assert!(blocked > 0, "el mapa 41 tiene celdas no movibles (muros)");
        eprintln!("mapa 41: {blocked} celdas bloqueadas / {water} agua (primeros 4M celdas)");
    }

    /// Celdas REALES del server_attr del mapa 41 (fijadas 2026-08-13 — el
    /// diagnóstico del bug de walkability): el parse es parity-correcto —
    /// la plaza del pueblo es movible, la FUENTE (bloque 2x2 junto al spawn)
    /// está marcada ATTR_BLOCK por el archivo, el agua NO bloquea y una
    /// montaña sí. Estas celdas fijan el contrato del parse (bits + layout).
    #[test]
    fn server_attr_map41_known_cells() {
        let map_path = "../../deploy/main/srv1/share/locale/spain/map";
        let attr = Path::new(map_path)
            .join("metin2_map_c1")
            .join("server_attr");
        if !attr.exists() {
            eprintln!("SKIP server_attr_map41_known_cells: runtime sin deploy");
            return;
        }
        let bytes = std::fs::read(&attr).expect("server_attr legible");
        let (tw, th, cells) = parse_attr_grid(&bytes, 41).expect("grid");
        let m = MapData {
            setting: map_41_setting(),
            tiles_w: tw,
            tiles_h: th,
            cells,
        };

        // Spawn del jugador (Town.txt -> base + (480,736)*100): movible.
        assert!(m.is_movable(969_600, 278_400), "spawn movible");

        // LA FUENTE del pueblo: celdas locales (62-63, 62-63) del tile (7,11)
        // — el primer MOVE del jugador hacia el norte (969595,278398) cae en
        // ella. ATTR_BLOCK REAL del archivo (el C++ la veria igual:
        // sectree_manager.cpp:760) — el bug 2026-08-13 NO era el parse.
        assert!(!m.is_movable(969_575, 278_375), "fuente: ATTR_BLOCK real");
        assert!(
            !m.is_movable(969_595, 278_398),
            "borde de la fuente (el MOVE rechazado)"
        );

        // AGUA (bit ATTR_WATER sin ATTR_BLOCK): el jugador puede caminar por
        // el agua (parity IsMovablePosition — solo BLOCK|OBJECT bloquean).
        let (wx, wy) = (996_300i32, 226_500i32);
        assert_eq!(
            m.attr(wx, wy),
            Some(0x42),
            "agua: WATER + bit meta, sin BLOCK"
        );
        assert!(m.is_movable(wx, wy), "el agua no bloquea");

        // MONTANA (ATTR_BLOCK): no movible (el anti-teleport la rechaza).
        let (mx, my) = (921_600i32, 204_800i32);
        assert_eq!(m.attr(mx, my), Some(0x49), "montana: BLOCK + bits meta");
        assert!(!m.is_movable(mx, my), "montana bloqueada");
    }
    /// Carga completa por el `MapStore` (index + Setting + attr) y fallos
    /// cacheados: un mapa inexistente no re-intenta por MOVE.
    #[test]
    fn map_store_load_and_cached_failure() {
        let map_path = "../../deploy/main/srv1/share/locale/spain/map";
        let mut store = MapStore::new();
        if Path::new(map_path).join("index").exists() {
            store.load(map_path, 41).expect("mapa 41 del index");
            assert!(store.get(41).is_some());
            let err = store.load(map_path, 999_999).expect_err("mapa inexistente");
            assert!(matches!(
                err,
                MapLoadError::Setting { .. } | MapLoadError::Index(_)
            ));
            let err2 = store.load(map_path, 999_999).expect_err("fallo cacheado");
            assert!(
                matches!(err2, MapLoadError::Cached { .. }),
                "no re-lee disco: {err2}"
            );
        } else {
            eprintln!("SKIP map_store_load_and_cached_failure: {map_path}/index no existe");
        }
    }
}
