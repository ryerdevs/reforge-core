//! F5: sistema de spawn de NPCs/mobs del mapa.
//!
//! Tres piezas (parity C++ verificada contra el runtime real, 2026-08-11):
//!
//! 1. `load_map_spawns` — extracción de los spawns del mapa desde los
//!    archivos del runtime, la MISMA fuente que el core C++:
//!    `SECTREE_MANAGER::Build` (sectree_manager.cpp:654-741) carga
//!    `{map_path}/index` (map id -> nombre), `{map_path}/{name}/Setting.txt`
//!    (BasePosition) y los 4 archivos de spawn
//!    `{map_path}/{name}/{regen,npc,boss,stone}.txt` vía `regen_load`
//!    (regen.cpp:604-713), cuyo tokenizer es `read_line`/`get_word`
//!    (regen.cpp:27-237). Este módulo replica el parseo EXACTO (token
//!    stream, no líneas: el C++ ignora líneas en blanco y los comentarios
//!    `//` saltan al fin de línea; un EOF corta el archivo).
//!
//! 2. La EXPANSIÓN de grupos (`load_groups` + expansión dentro de
//!    `load_map_spawns`): las entradas `g`/`r` referencian grupos
//!    (`group.txt`/`group_group.txt` del runtime, cargados por
//!    `CMobManager::LoadGroup`/`LoadGroupGroup`, mob_manager.cpp:246-379).
//!    El C++ expande en el spawn: `g` -> `SpawnGroup` -> los miembros del
//!    grupo (líder + líneas 1..n, CADA aparición = un mob); `r` ->
//!    `SpawnGroupGroup` -> `GetGroupFromGroupGroup` = UN grupo del gg
//!    ponderado-aleatorio por `prob` (`GetMember`, mob_manager.h:72-84) ->
//!    sus miembros. `load_map_spawns` devuelve la lista EXPANDIDA a mobs
//!    directos: las entradas `g`/`r` desaparecen y en su lugar van los
//!    vnums MIEMBRO (kind `Mob`) en la posición del entry.
//!
//! 3. `entry_spawns` — paquetes del world entry para las entradas ya
//!    resueltas (entry + `MobRow`), parity de `EncodeInsertPacket`
//!    (char.cpp:876-949).
//!
//! ## TRAP del vnum (colisión group/mob) — RESUELTO por la expansión
//!
//! Los vnums de grupo y de mob COLISIONAN (p.ej. en el mapa 41 el 101 es a
//! la vez el group-group `a1_01` y el mob `Perro Salvaje`). Antes de la
//! expansión, resolver una entrada `r` 101 vía `MobRepo::load_by_vnum(101)`
//! spawneaba un Perro Salvaje suelto en vez de la manada. La expansión
//! elimina la ambigüedad: los vnums de grupo NUNCA se emiten — solo los
//! miembros resueltos (kind `Mob` final). `entry_spawns` conserva la guarda
//! defensiva por si un caller le pasa un kind de grupo.
//!
//! ## Desviación documentada (grupos ponderados)
//!
//! El C++ elige UN grupo por entrada `r` (ponderado por `prob`,
//! `mob_manager.h:72-84`) — el spawn es aleatorio. La expansión F5 emite
//! TODOS los grupos de cada gg (fauna completa del mapa, determinista): el
//! runtime de respawn/combat F5 puede muestrear por peso más adelante.

use database::npc::MobRow;
use protocol::{
    TPacketGCCharacterAdd, TPacketGCCharacterAdditionalInfo, CHARACTER_NAME_MAX_LEN,
    CHR_EQUIPPART_NUM,
};

/// `CHAR_TYPE_NPC` — `length.h:330` (enum ECharType: MONSTER=0, **NPC=1**,
/// STONE=2, WARP=3, DOOR=4, BUILDING=5, PC=6). Verificado en el cliente:
/// `CActorInstance::EType` (`ActorInstance.h:74-88`) — TYPE_ENEMY=0,
/// TYPE_NPC=1 ... El wire `bType` del spawn = `mob_proto.type` (parity
/// `GetCharType()` = `m_bCharType` = `t->bType`, `char.cpp SetProto`).
pub const CHAR_TYPE_NPC: u8 = 1;

/// Tipo de entrada del regen (parity `REGEN_TYPE_*`, regen.h:3-11).
///
/// `Group`/`GroupGroup` solo existen DENTRO del parseo: `load_map_spawns`
/// los expande a `Mob` — el resultado NUNCA contiene kinds de grupo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnKind {
    /// `m` — un mob directo (`SpawnMob`/`SpawnMobRange`, char_manager.cpp);
    /// también el kind final de los miembros expandidos de grupos.
    Mob,
    /// `g` — un grupo de mobs (`SpawnGroup` — vnum = grupo de `group.txt`).
    /// Interno: se expande a sus miembros.
    Group,
    /// `r` — un grupo de grupos (`SpawnGroupGroup` — vnum de `group_group.txt`).
    /// Interno: se expande a los miembros de sus grupos.
    GroupGroup,
    /// `s` — un mob a POSICIÓN ALEATORIA del mapa (`SpawnMobRandomPosition`,
    /// char_manager.cpp:253-345). El extractor lo ancla en el punto de spawn
    /// del Town.txt (único ancla estable — GAP documentado).
    Anywhere,
}

/// Una entrada de spawn del mapa (parity del regen + base del Setting).
/// El resultado de `load_map_spawns` es una lista EXPANDIDA: cada entrada
/// es un mob directo (`kind = Mob`/`Anywhere`), con `count` copias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnEntry {
    /// vnum del MOB (siempre directo en la salida de `load_map_spawns`).
    pub vnum: u32,
    /// Centro del rect del regen en UNITS (parity `regen_load`: `sx`/`sy`
    /// del token + base del Setting).
    pub x: i32,
    pub y: i32,
    /// Media anchura del rect del regen en CELDAS (`ex`/`ey` de la línea —
    /// en esta variante son la mitad del tamaño del rect, no la otra
    /// esquina): cada copia se posiciona ALEATORIAMENTE en
    /// `[x - w_x*100, x + w_x*100] × [y - w_y*100, y + w_y*100]` (parity
    /// `SpawnMobRange`, char_manager.cpp:545-563 — `number(sx, ex)` sobre
    /// el rect; C28: el jitter del spawn es lo que evita que las copias
    /// del mismo entry nazcan apiladas). 0 = punto exacto (tests).
    pub w_x: i32,
    pub w_y: i32,
    /// Número de copias que el C++ spawnea: `max_count` del regen para los
    /// directos (`regen_spawn`, regen.cpp:322-380); para los miembros
    /// expandidos, `max_count × apariciones` del miembro en el grupo
    /// (cada aparición en la lista del grupo = un mob).
    pub count: u32,
    /// Intervalo de respawn del regen (segundos — token `time` de la línea;
    /// 0 = sin intervalo: el respawn por tiempo usa el default 60 s).
    /// Parity del `regen->time` (regen.cpp:690 — `event_create(regen_event,
    /// info, PASSES_PER_SEC(regen->time))`: el C++ respawnea cada `time`
    /// segundos; C23 lo usa como delay de respawn del mob muerto).
    pub time: u32,
    pub kind: SpawnKind,
}

/// Extrae los spawns del mapa desde el runtime, EXPANDIDOS a mobs directos
/// (parity `SECTREE_MANAGER::Build` + `regen_load` + `SpawnGroup`/
/// `SpawnGroupGroup`, sectree_manager.cpp:654-741 + char_manager.cpp:545-613).
///
/// - `map_id`: el índice del mapa (41 = `metin2_map_c1` en el runtime).
/// - `map_path`: la raíz de mapas del runtime — la que el core usa es
///   `<base>/share/locale/spain/map` (locale activo del core, syslog
///   "LoadSettings(locale/spain/...)"); el runtime real:
///   `/home/m2/source/metin2_svfiles/main/srv1/share/locale/spain/map`
///   (WSL; desde Windows: `\\wsl$\Debian-M2\home\m2\source\...`).
///
/// Semántica replicada:
/// - `index`: líneas `id nombre` (split por whitespace; `#`/`/` = comentario,
///   parity Build:669-678). Mapa inexistente -> `Err`.
/// - `Setting.txt`: `BasePosition x y` (parity `LoadSettingFile`).
/// - `Town.txt`: `x y` (parity `LoadMapRegion` — primer `fscanf(" %d %d ")`);
///   el posSpawn del mapa (solo se usa para las entradas `Anywhere`).
/// - Archivos de spawn en el ORDEN del C++: regen.txt, npc.txt, boss.txt,
///   stone.txt (Build:721-731). Archivo ausente -> se omite (parity: el C++
///   loguea y sigue — el return de `regen_load` no se verifica).
/// - Entradas con `time == 0` se omiten (parity regen_load:682-693: el C++
///   NO las spawnea al boot — `if (regen->time != 0)`); `e` (exception, zonas
///   sin spawn) se omiten; tipos desconocidos -> `Err` (parity exit(1)).
/// - EXPANSIÓN: `g` -> miembros del grupo (líder + 1..n, cada aparición =
///   un mob, agregados en `count`); `r` -> TODOS los grupos del gg -> sus
///   miembros (desviación documentada: el C++ elige UN grupo ponderado por
///   `prob`, mob_manager.h:72-84). Grupo/gg inexistente -> la entrada se
///   omite (parity: `NOT_EXIST_GROUP_VNUM`, char_manager.cpp:555-566).
/// - Las tablas de grupos se cargan de `{map_path}/../group.txt` y
///   `{map_path}/../group_group.txt` (parity `LocaleService_GetBasePath()`,
///   mob_manager.cpp:92-95); ausentes -> `Err` (parity: el C++ hace
///   `thecore_shutdown()`).
/// - El resultado contiene SOLO `kind = Mob`/`Anywhere` (la colisión de
///   vnums queda resuelta: los vnums de grupo no se emiten — ver doc módulo).
pub fn load_map_spawns(map_id: u32, map_path: &str) -> Result<Vec<SpawnEntry>, String> {
    let index = read_lossy(&format!("{map_path}/index"))
        .map_err(|e| format!("map index {map_path}/index: {e}"))?;
    let map_name = index
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with(['/', '#']))
        .find_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next()) {
                (Some(id), Some(name)) if id.parse::<u32>().ok() == Some(map_id) => Some(name),
                _ => None,
            }
        })
        .ok_or_else(|| format!("map {map_id} no está en {map_path}/index"))?;

    let dir = format!("{map_path}/{map_name}");
    let setting = read_lossy(&format!("{dir}/Setting.txt"))
        .map_err(|e| format!("{dir}/Setting.txt: {e}"))?;
    let base = setting
        .lines()
        .find_map(|l| {
            let mut it = l.split_whitespace();
            match (it.next(), it.next(), it.next()) {
                (Some(k), Some(x), Some(y)) if k.eq_ignore_ascii_case("BasePosition") => {
                    Some((x.parse::<i32>().ok()?, y.parse::<i32>().ok()?))
                }
                _ => None,
            }
        })
        .ok_or_else(|| format!("{dir}/Setting.txt sin BasePosition"))?;

    // Town.txt: el posSpawn del mapa (solo para entradas Anywhere — parity
    // LoadMapRegion:295-340: posSpawn = base + (x*100, y*100)).
    let town_spawn = read_lossy(&format!("{dir}/Town.txt"))
        .ok()
        .and_then(|t| {
            let mut it = t.split_whitespace();
            let x: i32 = it.next()?.parse().ok()?;
            let y: i32 = it.next()?.parse().ok()?;
            Some((base.0 + x * 100, base.1 + y * 100))
        })
        .unwrap_or(base);

    // Tablas de grupos del runtime (parity CMobManager::LoadGroup/LoadGroupGroup
    // — el C++ las carga SIEMPRE al boot y muere si faltan).
    let (groups, ggs) = load_groups(map_path)?;

    let mut out = Vec::new();
    for file in ["regen.txt", "npc.txt", "boss.txt", "stone.txt"] {
        let path = format!("{dir}/{file}");
        // Los archivos de spawn llevan comentarios en CP949 (no-UTF-8): se
        // leen como BYTES y se decodifican lossy — el C++ lee bytes crudos
        // (fopen "rt" + fgetc); los caracteres no-ASCII solo viven en los
        // comentarios (tokens que se saltan), el resto es ASCII puro.
        let Ok(text) = read_lossy(&path) else {
            // Parity: regen_load con archivo ausente loguea y el Build sigue.
            continue;
        };
        let tokens = regen_tokens(&text);
        let mut i = 0;
        while let Some(raw) = parse_regen_record(&tokens, &mut i)? {
            if raw.time == 0 {
                continue; // parity regen_load:682-693 (no spawnea al boot)
            }
            // Centro del rect en UNITS con la base del Setting (parity
            // regen_load:642-646 + 648-660: offset base y swap de extremos;
            // el centro es estable: (sx+ex)/2 = base + c*100).
            let (x, y) = match raw.kind {
                SpawnKind::Anywhere => town_spawn,
                _ => (base.0 + raw.c_x * 100, base.1 + raw.c_y * 100),
            };
            // EXPANSIÓN de grupos: `m`/`s` ya son directos; `g`/`r` se
            // resuelven a sus miembros (cada aparición = un mob, agregado
            // en count dentro de esta entrada).
            let (members, kind) = match raw.kind {
                SpawnKind::Mob | SpawnKind::Anywhere => (
                    vec![(raw.vnum as u32, raw.max_count as u32)],
                    raw.kind,
                ),
                SpawnKind::Group => {
                    let Some(g) = groups.get(&(raw.vnum as u32)) else {
                        continue; // parity NOT_EXIST_GROUP_VNUM (sin spawn)
                    };
                    (aggregate_members(&g.members, raw.max_count as u32), SpawnKind::Mob)
                }
                SpawnKind::GroupGroup => {
                    let Some(gg) = ggs.get(&(raw.vnum as u32)) else {
                        continue; // parity NOT_EXIST_GROUP_GROUP_VNUM
                    };
                    let mut acc: Vec<(u32, u32)> = Vec::new();
                    for (gv, _prob) in &gg.groups {
                        let Some(g) = groups.get(gv) else {
                            continue; // parity NOT_EXIST_GROUP_VNUM (grupo)
                        };
                        acc.extend(aggregate_members(&g.members, raw.max_count as u32));
                    }
                    (acc, SpawnKind::Mob)
                }
            };
            for (vnum, count) in members {
                out.push(SpawnEntry { vnum, x, y, count, kind, w_x: raw.w_x, w_y: raw.w_y, time: raw.time });
            }
        }
    }
    Ok(out)
}

/// Agrega las apariciones de los miembros de un grupo: cada aparición en la
/// lista [líder, 1..n] = un mob (parity `SpawnGroup` — SpawnMobRange por
/// miembro, char_manager.cpp:588-598). `count` del entry = copias del grupo.
fn aggregate_members(members: &[u32], count: u32) -> Vec<(u32, u32)> {
    let mut acc: Vec<(u32, u32)> = Vec::new();
    for &m in members {
        match acc.iter_mut().find(|(v, _)| *v == m) {
            Some((_, c)) => *c += count,
            None => acc.push((m, count)),
        }
    }
    acc
}

/// Caché de filas de `mob_proto` (vnum -> `MobRow`) para la resolución de
/// spawns. `mob_proto` no cambia entre entradas al mundo: la caché convierte
/// la resolución de ~10.000 conexiones PG (una por spawn — el stall de
/// minutos medido en la integración) en UNA query batch por los vnums que
/// falten (117 para el mapa 41 — los grupos ya expandidos).
///
/// Invaldación: `clear()` — el NOTIFY/reload de `mob_proto` (hot reload,
/// plan §5.6) NO está cableado todavía; cuando llegue, el listener del canal
/// hace `cache.lock().await.clear()` (documentado — `mob_proto` es estático
/// en el runtime actual; el canal comparte la caché entre conexiones).
#[derive(Debug, Default)]
pub struct MobCache {
    rows: std::collections::HashMap<i64, MobRow>,
}

impl MobCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Invalida toda la caché (reload/NOTIFY de `mob_proto` — documentado).
    pub fn clear(&mut self) {
        self.rows.clear();
    }

    /// Resuelve los spawns a pares `(SpawnEntry, MobRow)` con la caché +
    /// UNA query batch (`MobRepo::load_by_vnums`) por los vnums que falten.
    /// Solo las entradas spawnables (kind `Mob`/`Anywhere` — los grupos ya
    /// se expandieron; la guarda es defensiva). Los vnums sin fila en
    /// `mob_proto` se omiten (parity: `SpawnMob` -> nullptr).
    pub async fn resolve(
        &mut self,
        repo: &database::npc::MobRepo,
        spawns: &[SpawnEntry],
    ) -> Result<Vec<(SpawnEntry, MobRow)>, String> {
        let missing = missing_vnums(spawns, &self.rows);
        if !missing.is_empty() {
            self.rows.extend(repo.load_by_vnums(&missing).await?);
        }
        Ok(spawns
            .iter()
            .filter(|e| matches!(e.kind, SpawnKind::Mob | SpawnKind::Anywhere))
            .filter_map(|e| self.rows.get(&i64::from(e.vnum)).map(|row| (*e, row.clone())))
            .collect())
    }
}

/// Vnums DISTINTOS de las entradas spawnables que NO están en la caché
/// (los que la resolución debe cargar en el batch).
fn missing_vnums(spawns: &[SpawnEntry], cache: &std::collections::HashMap<i64, MobRow>) -> Vec<i64> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for e in spawns {
        if !matches!(e.kind, SpawnKind::Mob | SpawnKind::Anywhere) {
            continue;
        }
        let v = i64::from(e.vnum);
        if cache.contains_key(&v) || !seen.insert(v) {
            continue;
        }
        out.push(v);
    }
    out
}

/// Un grupo de mobs (parity `CMobGroup`, mob_manager.h:94-122): miembros en
/// orden [líder, líneas 1..n] — cada aparición cuenta como un mob.
#[derive(Debug, Clone, PartialEq, Eq)]
struct MobGroup {
    vnum: u32,
    members: Vec<u32>,
}

/// Un grupo-de-grupos (parity `CMobGroupGroup`, mob_manager.h:50-92):
/// grupos miembros con su `prob` (0 = omitido — `AddMember`, :59-69).
#[derive(Debug, Clone, PartialEq, Eq)]
struct MobGroupGroup {
    vnum: u32,
    groups: Vec<(u32, u32)>,
}

/// Carga las tablas de grupos del runtime (parity `CMobManager::LoadGroup`/
/// `LoadGroupGroup`, mob_manager.cpp:246-379): `{map_path}/../group.txt` y
/// `{map_path}/../group_group.txt` — el base path del locale (mob_manager.cpp:
/// 92-95 `LocaleService_GetBasePath()`), un nivel ARRIBA de `map/`.
///
/// Formato (parity `CTextFileLoader::LoadGroup`, text_file_loader.cpp:56-148
/// + `SplitLine`, file_loader.cpp): nodos `Group <name>` con cuerpo
/// `{ key valor... }`; claves en minúscula (stl_lowers); tokens entre
/// comillas `"..."`; `#` al inicio de línea = comentario; líneas en blanco
/// ignoradas. `group.txt`: `leader <name> <vnum>` + `vnum <id>` + `k <name>
/// <vnum>`; `group_group.txt`: `vnum <id>` + `k <grupo> <prob>`.
/// Archivos ausentes -> `Err` (parity: `thecore_shutdown()` si Load falla).
fn load_groups(map_path: &str) -> Result<(std::collections::HashMap<u32, MobGroup>, std::collections::HashMap<u32, MobGroupGroup>), String> {
    let mut groups = std::collections::HashMap::new();
    let mut ggs = std::collections::HashMap::new();

    let group_text = read_lossy(&format!("{map_path}/../group.txt"))
        .map_err(|e| format!("{map_path}/../group.txt: {e}"))?;
    for (_name, kv) in parse_loader_nodes(&group_text) {
        let Some(vnum) = kv.get("vnum").and_then(|t| t.first()) else {
            continue; // parity: sys_err "no vnum, node" y sigue
        };
        let Ok(vnum) = vnum.parse::<u32>() else { continue };
        let Some(leader) = kv.get("leader") else {
            continue; // parity: sys_err "no leader" y sigue
        };
        let Some(lv) = leader.get(1) else {
            continue; // parity: pTok->size() < 2 -> skip
        };
        let Ok(lv) = lv.parse::<u32>() else { continue };
        let mut members = vec![lv];
        let mut k = 1;
        while let Some(toks) = kv.get(&k.to_string()) {
            if let Some(v) = toks.get(1).and_then(|t| t.parse::<u32>().ok()) {
                members.push(v);
            }
            k += 1;
        }
        groups.insert(vnum, MobGroup { vnum, members });
    }

    let gg_text = read_lossy(&format!("{map_path}/../group_group.txt"))
        .map_err(|e| format!("{map_path}/../group_group.txt: {e}"))?;
    for (_name, kv) in parse_loader_nodes(&gg_text) {
        let Some(vnum) = kv.get("vnum").and_then(|t| t.first()) else {
            continue; // parity: sys_err "no vnum" y sigue
        };
        let Ok(vnum) = vnum.parse::<u32>() else { continue };
        let mut gg = MobGroupGroup { vnum, groups: Vec::new() };
        let mut k = 1;
        while let Some(toks) = kv.get(&k.to_string()) {
            let gv = toks.first().and_then(|t| t.parse::<u32>().ok());
            let prob = toks.get(1).and_then(|t| t.parse::<u32>().ok()).unwrap_or(1);
            if let Some(gv) = gv
                && prob != 0 {
                    gg.groups.push((gv, prob)); // parity AddMember prob==0 skip
                }
            k += 1;
        }
        ggs.insert(vnum, gg);
    }

    Ok((groups, ggs))
}

/// Parseo de nodos del loader de texto (parity `CTextFileLoader::LoadGroup`,
/// text_file_loader.cpp:56-148): devuelve (nombre, {clave: tokens}) por nodo
/// `Group <name> { ... }`. Claves en minúscula; `{`/`}` delimitan; líneas
/// vacías y comentarios `#` se ignoran (parity `SplitLine`).
fn parse_loader_nodes(text: &str) -> Vec<(String, std::collections::HashMap<String, Vec<String>>)> {
    let mut nodes = Vec::new();
    let mut cur: Option<(String, std::collections::HashMap<String, Vec<String>>)> = None;
    for line in text.lines() {
        let Some(toks) = loader_line_tokens(line) else { continue };
        match toks[0].to_ascii_lowercase().as_str() {
            "{" => continue,
            "}" => {
                if let Some(node) = cur.take() {
                    nodes.push(node);
                }
            }
            "group" => {
                if let Some(node) = cur.take() {
                    nodes.push(node);
                }
                cur = Some((toks.get(1).cloned().unwrap_or_default(), std::collections::HashMap::new()));
            }
            key => {
                if let Some((_, kv)) = cur.as_mut()
                    && toks.len() > 1 {
                        kv.insert(key.to_string(), toks[1..].to_vec());
                    }
            }
        }
    }
    if let Some(node) = cur.take() {
        nodes.push(node);
    }
    nodes
}

/// Tokens de una línea del loader (parity `CMemoryTextFileLoader::SplitLine`):
/// tokens separados por whitespace; `"..."` agrupa (el contenido va sin
/// comillas); `#` al PRIMER token de la línea = comentario (None); línea
/// vacía = None.
fn loader_line_tokens(line: &str) -> Option<Vec<String>> {
    let mut toks = Vec::new();
    let mut i = 0;
    let b = line.as_bytes();
    let n = b.len();
    while i < n {
        while i < n && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= n {
            break;
        }
        if b[i] == b'#' {
            return if toks.is_empty() { None } else { Some(toks) };
        }
        if b[i] == b'"' {
            let start = i + 1;
            let mut end = start;
            while end < n && b[end] != b'"' {
                end += 1;
            }
            toks.push(line[start..end].to_string());
            i = if end < n { end + 1 } else { end };
        } else {
            let start = i;
            while i < n && !b[i].is_ascii_whitespace() {
                i += 1;
            }
            toks.push(line[start..i].to_string());
        }
    }
    if toks.is_empty() { None } else { Some(toks) }
}

/// Registro crudo de una línea regen (los 11 tokens; parity `read_line`
/// regen.cpp:89-237). `c_x`/`c_y` = centro en CELDAS del rect (el token sx/
/// sy); `w_x`/`w_y` = media anchura en celdas (los tokens ex/ey — en esta
/// variante la media anchura, no la otra esquina: `r 360 425 10 10` = rect
/// 20×20 celdas centrado en (360,425)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RegenRaw {
    kind: SpawnKind,
    c_x: i32,
    c_y: i32,
    w_x: i32,
    w_y: i32,
    z_section: u8,
    time: u32,
    max_count: i32,
    vnum: i32,
}

/// Parseo de registros del token stream (parity `read_line` — consume 11
/// tokens por registro; los tokens sobrantes de una línea corta continúan en
/// la siguiente: el C++ es agnóstico de líneas). `Ok(None)` = registro
/// exception (`e` — no es un spawn); `Err` = tipo desconocido (parity
/// exit(1)).
fn parse_regen_record(tokens: &[&str], i: &mut usize) -> Result<Option<RegenRaw>, String> {
    const MODE_TYPE: usize = 0;
    const MODE_SX: usize = 1;
    const MODE_SY: usize = 2;
    const MODE_EX: usize = 3;
    const MODE_EY: usize = 4;
    const MODE_Z: usize = 5;
    const MODE_DIR: usize = 6;
    const MODE_TIME: usize = 7;
    const MODE_PERCENT: usize = 8;
    const MODE_MAX_COUNT: usize = 9;
    const MODE_VNUM: usize = 10;

    let mut mode = MODE_TYPE;
    let mut kind = SpawnKind::Mob;
    let mut c_x = 0i32;
    let mut c_y = 0i32;
    let mut w_x = 0i32;
    let mut w_y = 0i32;
    let mut z_section = 0u8;
    let mut time = 0u32;
    let mut max_count = 0i32;

    while let Some(w) = tokens.get(*i).copied() {
        *i += 1;
        if w.starts_with("//") {
            continue; // comentario (el tokenizer ya cortó a fin de línea)
        }
        match mode {
            MODE_TYPE => {
                kind = match w.as_bytes()[0] {
                    b'm' => SpawnKind::Mob,
                    b'g' => SpawnKind::Group,
                    b'e' => return Ok(None), // exception: zona sin spawn
                    b'r' => SpawnKind::GroupGroup,
                    b's' => SpawnKind::Anywhere,
                    other => {
                        return Err(format!(
                            "regen: tipo desconocido '{other}' (token '{w}')"
                        ))
                    }
                };
                mode += 1;
            }
            MODE_SX => {
                c_x = parse_i32(w, "sx")?;
                mode += 1;
            }
            MODE_SY => {
                c_y = parse_i32(w, "sy")?;
                mode += 1;
            }
            MODE_EX => {
                w_x = parse_i32(w, "ex")?; // media anchura en celdas (C28)
                mode += 1;
            }
            MODE_EY => {
                w_y = parse_i32(w, "ey")?;
                mode += 1;
            }
            MODE_Z => {
                z_section = parse_u8(w, "z_section")?;
                mode += 1;
            }
            MODE_DIR => mode += 1, // dirección: no participa (GAP rotación)
            MODE_TIME => {
                time = parse_time(w)?;
                mode += 1;
            }
            MODE_PERCENT => mode += 1, // el C++ lo consume sin usarlo
            MODE_MAX_COUNT => {
                max_count = parse_i32(w, "max_count")?;
                mode += 1;
            }
            MODE_VNUM => {
                let _ = z_section; // conservado para parity futura (punto z)
                return Ok(Some(RegenRaw {
                    kind,
                    c_x,
                    c_y,
                    w_x,
                    w_y,
                    z_section,
                    time,
                    max_count,
                    vnum: parse_i32(w, "vnum")?,
                }));
            }
            _ => unreachable!("modo regen fuera de 0..=10"),
        }
    }
    Ok(None) // EOF (parity get_word -> read_line false)
}

/// Lectura de archivos del runtime como bytes + decodificación lossy (los
/// archivos de spawn llevan comentarios CP949 — no-UTF-8; el C++ lee bytes
/// crudos con `fgetc`). Los tokens reales (tipo/números) son ASCII puro: la
/// sustitución U+FFFD solo afecta a comentarios que el parseo descarta.
fn read_lossy(path: &str) -> std::io::Result<String> {
    Ok(String::from_utf8_lossy(&std::fs::read(path)?).into_owned())
}

/// Tokenizer del regen (parity `get_word`, regen.cpp:27-78): palabras
/// separadas por whitespace; `"` inicia una palabra entre comillas; una
/// palabra que empieza con `//` corta al fin de línea (comentario inline);
/// EOF termina. Las líneas en blanco son transparentes (el C++ salta el
/// whitespace y sigue leyendo la siguiente palabra).
fn regen_tokens(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let b = text.as_bytes();
    let mut i = 0;
    let n = b.len();
    while i < n {
        let c = b[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let start = i;
        if c == b'"' {
            // palabra entre comillas (semicolon_mode del C++): el contenido
            // va SIN las comillas (parity get_word: la apertura se salta y
            // la de cierre termina la palabra). El caso patológico de un
            // comentario dentro de comillas no se replica (no ocurre en el
            // runtime — documentado).
            let content_start = i + 1;
            let mut end = content_start;
            while end < n && b[end] != b'"' {
                end += 1;
            }
            i = if end < n { end + 1 } else { end }; // salta la de cierre
            out.push(&text[content_start..end]);
            continue;
        }
        // comentario `//`: corta al fin de línea (parity i==2 && "//")
        if i + 1 < n && c == b'/' && b[i + 1] == b'/' {
            while i < n && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        while i < n && !b[i].is_ascii_whitespace() {
            i += 1;
        }
        out.push(&text[start..i]);
    }
    out
}/// Parseo decimal estricto de un token del regen (parity `str_to_number`).
fn parse_i32(w: &str, what: &str) -> Result<i32, String> {
    w.parse().map_err(|_| format!("regen: {what} '{w}' no es un entero"))
}

fn parse_u8(w: &str, what: &str) -> Result<u8, String> {
    w.parse().map_err(|_| format!("regen: {what} '{w}' no es un byte"))
}

/// Tiempo del regen (parity regen.cpp:184-217): dígitos acumulados y
/// unidades `h`/`m`/`s`; SIN unidad el valor queda 0 (el C++ no spawnea al
/// boot con time == 0).
fn parse_time(w: &str) -> Result<u32, String> {
    let mut total = 0u32;
    let mut acc = 0u32;
    for ch in w.bytes() {
        match ch {
            b'0'..=b'9' => {
                acc = acc
                    .checked_mul(10)
                    .and_then(|a| a.checked_add(u32::from(ch - b'0')))
                    .ok_or_else(|| format!("regen: time '{w}' desborda"))?;
            }
            b'h' => {
                total = total.checked_add(acc * 3600).ok_or_else(|| format!("regen: time '{w}' desborda"))?;
                acc = 0;
            }
            b'm' => {
                total = total.checked_add(acc * 60).ok_or_else(|| format!("regen: time '{w}' desborda"))?;
                acc = 0;
            }
            b's' => {
                total = total.checked_add(acc).ok_or_else(|| format!("regen: time '{w}' desborda"))?;
                acc = 0;
            }
            other => return Err(format!("regen: time '{w}': carácter '{other}' inválido")),
        }
    }
    Ok(total)
}

/// Paquetes de spawn de las entradas resueltas (entry + su `MobRow`).
///
/// FIRMA PARA EL INTEGRADOR (channel): `mobs` = pares (entrada de
/// `load_map_spawns` — SIEMPRE kind `Mob`/`Anywhere` tras la expansión,
/// ver doc del módulo —, fila de `MobRepo::load_by_vnum` del vnum de la
/// entrada). `vid_base` = primer VID libre del canal (parity del pool
/// global `AllocVID`, char_manager.cpp:79-83 — el contador del C++ empieza
/// en 1 y avanza por mob creado). Devuelve los paquetes EN ORDEN de emisión.
///
/// Parity `char.cpp:876-949` (`EncodeInsertPacket`):
/// - Por cada una de las `count` copias del entry: `TPacketGCCharacterAdd`
///   (37 B) SIEMPRE; `TPacketGCCharacterAdditionalInfo` (70 B) SOLO si
///   `bType == CHAR_TYPE_NPC` (1) — el C++: `if (IsPC() || m_bCharType ==
///   CHAR_TYPE_NPC)` (char.cpp:922). Para monsters (type 0) el C++ NO manda
///   el addInfo: el cliente resuelve el nombre del pack por race
///   (`PythonNetworkStreamPhaseGameActor.cpp:132-143`).
/// - El ORDEN (add, [addInfo]) POR MOB es obligatorio: el cliente bufferiza
///   el add de NPCs en `s_kNetActorData` (slot ÚNICO global) y lo suelta con
///   el addInfo del MISMO VID (`PythonNetworkStreamPhaseGameActor.cpp:90-198`).
/// - `dwVID` = `vid_base + i` (i = índice de la copia; el caller avanza su
///   contador en Σ entry.count).
/// - `wRaceNum` = vnum del entry (`GetRaceNum()` = vnum para mobs).
/// - `bType` = `mob.b_type` (`GetCharType()` = `mob_proto.type`).
/// - x/y = UNITS del entry (centro del rect — el jitter es del runtime F5);
///   z = 0 (parity `SpawnMobRange` z=0; el punto usa z_section, 0 en el runtime).
/// - GAP del slice (documentados, mismo patrón que el mapeo de PCs F4):
///   `angle` = 0.0 (el C++ randomiza, `number(0,360)` o `(dir-1)*45`,
///   char_manager.cpp:417-427), speeds/state/afects = 0 (runtime F5),
///   addInfo: parts/empire/guild/level/alignment = 0 (NPCs: `dwLevel` = 0
///   sin ENABLE_SHOWNPCLEVEL, char.cpp:940-944; el empire del mapa lo
///   asigna el C++ `GetEmpireFromMapIndex`, char_manager.cpp:422-425).
/// - addInfo `name` = bytes crudos de `mob.locale_name` (el C++ manda
///   `GetName()` = `szLocaleName`, CP949; el cliente lo usa SOLO como
///   fallback para NPCs, multilang §17).
///
/// `map_id` no participa en el wire (parity: `EncodeInsertPacket` no lo
/// lleva); se mantiene en la firma por el contrato del lane (estado por
/// mapa del runtime F5).
pub fn entry_spawns(
    map_id: u32,
    mobs: &[(SpawnEntry, MobRow)],
    vid_base: u32,
) -> Vec<Vec<u8>> {
    let _ = map_id; // no participa en el wire (ver doc)
    let mut out = Vec::new();
    let mut vid = vid_base;
    for (entry, mob) in mobs {
        // TRAP grupos: SOLO Mob/Anywhere spawnean un mob directo (los vnums
        // de grupo colisionan con mobs reales — ver doc del módulo).
        if !matches!(entry.kind, SpawnKind::Mob | SpawnKind::Anywhere) {
            continue;
        }
        let Ok(b_type) = database::npc::wire_b_type(mob.b_type) else {
            continue; // defensivo: type fuera de BYTE no emite paquete
        };
        for i in 0..entry.count {
            out.extend(character_add_packets(entry, mob, vid + i, b_type));
        }
        vid += entry.count;
    }
    out
}

/// Paquetes de UNA copia del mob: add (37 B) [+ addInfo (70 B) si NPC].
fn character_add_packets(entry: &SpawnEntry, mob: &MobRow, vid: u32, b_type: u8) -> Vec<Vec<u8>> {
    let mut pkts = vec![
        TPacketGCCharacterAdd::new(
            vid,
            0.0, // GAP rotación (el C++ randomiza)
            entry.x,
            entry.y,
            0, // z (parity SpawnMobRange)
            b_type,
            entry.vnum,
            // b_moving_speed = move_speed de la tabla (parity char.cpp:2257
            // `SetPoint(POINT_MOV_SPEED, sMovingSpeed)` → GetLimitPoint →
            // wire BYTE, truncación igual que el cast (BYTE) del C++). El
            // cliente lo usa para la animación del paso (SetMoveSpeed(x/100)).
            mob.move_speed as u8,
            // C29: b_attack_speed = attack_speed de la tabla (parity
            // char.cpp:2245-2246 — `SetPoint(POINT_ATT_SPEED, sAttackSpeed)`;
            // el cliente lo usa para la animación del golpe). Era 0 fijo
            // (GAP cerrado con la selección de la columna).
            mob.attack_speed as u8,
            0, // b_state_flag (runtime F5)
            [0, 0], // dw_affect_flag (runtime F5)
        )
        .to_bytes()
        .to_vec(),
    ];
    if b_type == CHAR_TYPE_NPC {
        // name = bytes crudos del locale_name (parity GetName() = szLocaleName;
        // copia C con NUL — max 24 bytes, el resto a cero).
        let mut name = [0u8; CHARACTER_NAME_MAX_LEN + 1];
        let n = mob.locale_name.len().min(CHARACTER_NAME_MAX_LEN);
        name[..n].copy_from_slice(&mob.locale_name[..n]);
        pkts.push(
            TPacketGCCharacterAdditionalInfo {
                header: TPacketGCCharacterAdditionalInfo::HEADER,
                dw_vid: vid,
                name,
                aw_part: [0; CHR_EQUIPPART_NUM],
                b_empire: 0, // GAP: empire del mapa
                dw_guild_id: 0,
                dw_level: 0, // parity NPCs sin ENABLE_SHOWNPCLEVEL
                s_alignment: 0,
                b_pk_mode: 0,
                dw_mount_vnum: 0,
                dw_arrow: 0,
            }
            .to_bytes()
            .to_vec(),
        );
    }
    pkts
}

#[cfg(test)]
mod tests {
    use super::*;
    use database::npc::MobRow;

    /// Base real del mapa 41 (Setting.txt del runtime, verificado 2026-08-11:
    /// `BasePosition 921600 204800`, CellScale 200).
    const MAP41_BASE: (i32, i32) = (921600, 204800);
    /// Town del mapa 41 (Town.txt: `480 736`) -> posSpawn en UNITS.
    const MAP41_TOWN: (i32, i32) = (969600, 278400);

    fn mob_row(vnum: i64, b_type: i16, locale_name: &[u8]) -> MobRow {
        MobRow {
            vnum,
            name: "test".into(),
            locale_name: locale_name.to_vec(),
            b_type,
            battle_type: 0,
            level: 1,
            size: "SMALL".into(),
            ai_flag: None,
            folder: String::new(),
            // F5.2 (integración): campos de combate (valores del mob 101).
            ht: 5,
            def: 4,
            max_hp: 126,
            attack_range: 175,
            // F5.3: recompensas del mob 101 (mob_proto — exp/gold).
            exp: 22,
            gold_min: 15,
            gold_max: 45,
            // F5.3: drop primario del mob 101 (mob_proto.drop_item).
            drop_item: 101,
            // F5.3: velocidad del mob (mob_proto.move_speed — UNITS/seg).
            move_speed: 100,
            // C29: velocidad de ataque del mob 101 (mob_proto.attack_speed).
            attack_speed: 100,
            // F5.3: daño del ataque del mob 101 (mob_proto.damage_min/max).
            damage_min: 3,
            damage_max: 8,
            // F5.3: aggro proactivo del mob 101 (mob_proto.aggressive_sight).
            aggressive_sight: 400,
        }
    }

    /// Entradas REALES del runtime del mapa 41 (npc.txt / regen.txt del
    /// deploy — copiadas byte a byte 2026-08-11).
    const NPC_LINES: &str = "\
// �����NPC
m\t444\t623\t0\t0\t0\t7\t1m\t100\t1\t20340
m\t443\t644\t0\t0\t0\t7\t1m\t100\t1\t20344
m\t343\t560\t1\t1\t0\t0\t1m\t100\t1\t20002

// �ٳ��� -----------------------------------------------------------------------------------
s\t0\t0\t0\t0\t0\t0\t600s\t100\t10\t5004
";
    const REGEN_LINE: &str = "r\t360\t425\t10\t10\t0\t0\t5s\t100\t1\t101\n";

    /// Fixture de las tablas de grupos (formato REAL del runtime — nodos del
    /// CTextFileLoader, con líder con nombre entre comillas como en group.txt).
    const GROUP_TXT: &str = "\
Group\tL01_Perros
{
\tLeader\tWildhund\t101
\tVnum\t101
\t1\tWildhund\t101
\t2\tWildhund\t101
\t3\tFuchs\t2101
}
Group\tL30_Boss
{
\tLeader\tLykos\t191
\tVnum\t12007
\t1\tPony\t20029
\t2\tPony\t20029
}
Group\tHambrientos
{
\tLeader\t\"Hungriger Wildhund\"\t171
\tVnum\t171
}
";
    const GROUP_GROUP_TXT: &str = "\
Group\ta1_01
{
\tVnum\t101
\t1\t101\t1
\t2\t171\t1
}
Group\tdeposito
{
\tVnum\t2001
\t1\t12007\t1
}
";

    /// Crea el FS de prueba: `map_path` = `<tmp>/<pid>_<tag>/map`; las tablas
    /// de grupos viven en `<tmp>/<pid>_<tag>/group*.txt` (parity: un nivel
    /// ARRIBA de `map/` — `LocaleService_GetBasePath()`, mob_manager.cpp:92-95).
    /// El tag evita colisiones entre tests paralelos (mismo proceso).
    fn fixture_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("m2_f5_spawn_test_{}_{tag}", std::process::id()));
        let map = dir.join("map").join("metin2_map_c1");
        std::fs::create_dir_all(&map).expect("tmp dir");
        std::fs::write(dir.join("map").join("index"), "1 metin2_map_a1\n41 metin2_map_c1\n43 metin2_map_c3\n")
            .expect("index");
        std::fs::write(
            map.join("Setting.txt"),
            "ScriptType\tMapSetting\n\nCellScale\t200\n\nMapSize\t4\t5\nBasePosition\t921600\t204800\n",
        )
        .expect("setting");
        std::fs::write(map.join("Town.txt"), "480 736\n125 1113\n125 1113\n480 736\n")
            .expect("town");
        std::fs::write(map.join("regen.txt"), REGEN_LINE).expect("regen");
        std::fs::write(map.join("npc.txt"), NPC_LINES).expect("npc");
        std::fs::write(map.join("boss.txt"), "g\t374\t1111\t100\t100\t0\t0\t1000s\t100\t1\t12007\n")
            .expect("boss");
        std::fs::write(dir.join("group.txt"), GROUP_TXT).expect("group");
        std::fs::write(dir.join("group_group.txt"), GROUP_GROUP_TXT).expect("group_group");
        dir
    }

    /// load_map_spawns contra un FS de prueba que replica el layout del
    /// runtime (index + Setting + Town + los 4 archivos de spawn + tablas de
    /// grupos), con la EXPANSIÓN: `r`/`g` -> miembros directos.
    #[test]
    fn load_map_spawns_parses_runtime_layout() {
        let dir = fixture_dir("layout");
        // stone.txt ausente -> se omite (parity: regen_load loguea y sigue).
        let result = load_map_spawns(41, dir.join("map").to_str().expect("utf8"));
        std::fs::remove_dir_all(&dir).ok();
        let entries = result.expect("load_map_spawns");
        // r 101 -> gg 101 -> grupos [101, 171]: 101 (3×), 2101 (1×), 171 (1×);
        // npc: 3 m directos + 1 s; boss g 12007 -> [191, 20029, 20029].
        assert_eq!(entries.len(), 9, "{entries:#?}");
        // Orden del C++: regen.txt, npc.txt, boss.txt (Build:721-731).
        // r 360 425 -> miembros en el centro (base + 360*100, base + 425*100).
        let e0 = entries[0];
        assert_eq!(
            e0,
            SpawnEntry { vnum: 101, x: 957600, y: 247300, count: 3, kind: SpawnKind::Mob, w_x: 10, w_y: 10, time: 5 },
            "Perro Salvaje 101: líder + 2 apariciones (líder + líneas 1..n); rect del regen + intervalo 5s"
        );
        assert_eq!(
            entries[1],
            SpawnEntry { vnum: 2101, x: 957600, y: 247300, count: 1, kind: SpawnKind::Mob, w_x: 10, w_y: 10, time: 5 }
        );
        assert_eq!(
            entries[2],
            SpawnEntry { vnum: 171, x: 957600, y: 247300, count: 1, kind: SpawnKind::Mob, w_x: 10, w_y: 10, time: 5 },
            "segundo grupo del gg (todos los grupos — fauna completa)"
        );
        // m 444 623 -> NPC 20340 en el centro del punto (sin rect ni time).
        assert_eq!(
            entries[3],
            SpawnEntry {
                vnum: 20340,
                x: MAP41_BASE.0 + 444 * 100,
                y: MAP41_BASE.1 + 623 * 100,
                count: 1,
                kind: SpawnKind::Mob,
                w_x: 0,
                w_y: 0,
                time: 60,
            }
        );
        // m 343 560 1 1 -> rect [342,344]x[559,561]: centro = 343/560 celdas,
        // media anchura 1 celda + intervalo 1m.
        assert_eq!(
            entries[5],
            SpawnEntry {
                vnum: 20002,
                x: MAP41_BASE.0 + 343 * 100,
                y: MAP41_BASE.1 + 560 * 100,
                count: 1,
                kind: SpawnKind::Mob,
                w_x: 1,
                w_y: 1,
                time: 60,
            }
        );
        // s 0 0 ... 600s 100 10 5004 -> Anywhere anclado en el Town spawn,
        // count 10 (el C++ spawnea 10 copias aleatorias del mapa).
        assert_eq!(
            entries[6],
            SpawnEntry {
                vnum: 5004,
                x: MAP41_TOWN.0,
                y: MAP41_TOWN.1,
                count: 10,
                kind: SpawnKind::Anywhere,
                w_x: 0,
                w_y: 0,
                time: 600,
            }
        );
        // g 12007 -> [191, 20029, 20029] agregados por vnum (rect 100 celdas
        // + intervalo 1000s del grupo).
        assert_eq!(
            entries[7],
            SpawnEntry { vnum: 191, x: 959000, y: 315900, count: 1, kind: SpawnKind::Mob, w_x: 100, w_y: 100, time: 1000 }
        );
        assert_eq!(
            entries[8],
            SpawnEntry { vnum: 20029, x: 959000, y: 315900, count: 2, kind: SpawnKind::Mob, w_x: 100, w_y: 100, time: 1000 },
            "2 apariciones de Pony en el grupo"
        );
        // El resultado NO contiene kinds de grupo (colisión resuelta).
        assert!(
            entries.iter().all(|e| matches!(e.kind, SpawnKind::Mob | SpawnKind::Anywhere)),
            "solo Mob/Anywhere: {entries:#?}"
        );
    }

    /// Grupo/gg inexistente en las tablas -> la entrada se omite (parity
    /// NOT_EXIST_GROUP_VNUM / NOT_EXIST_GROUP_GROUP_VNUM, char_manager.cpp:555-566).
    #[test]
    fn load_map_spawns_skips_missing_groups() {
        let dir = fixture_dir("missing");
        let map = dir.join("map").join("metin2_map_c1");
        // `r` con vnum 9999 (gg inexistente) y `g` con vnum 9998 (grupo
        // inexistente) — ambos se omiten; el `m` directo se conserva.
        std::fs::write(
            map.join("regen.txt"),
            "r\t1\t1\t0\t0\t0\t0\t5s\t100\t1\t9999\ng\t2\t2\t0\t0\t0\t0\t5s\t100\t1\t9998\nm\t3\t3\t0\t0\t0\t0\t5s\t100\t1\t20340\n",
        )
        .expect("regen");
        std::fs::write(map.join("npc.txt"), "").expect("npc");
        std::fs::write(map.join("boss.txt"), "").expect("boss");
        let result = load_map_spawns(41, dir.join("map").to_str().expect("utf8"));
        std::fs::remove_dir_all(&dir).ok();
        let entries = result.expect("load_map_spawns");
        assert_eq!(entries.len(), 1, "{entries:#?}");
        assert_eq!(entries[0].vnum, 20340, "solo el mob directo");
    }

    /// Tablas de grupos ausentes -> Err (parity: `thecore_shutdown()` en
    /// `CMobManager::Initialize`, mob_manager.cpp:97-106).
    #[test]
    fn load_map_spawns_missing_group_files_err() {
        let dir = fixture_dir("nofiles");
        std::fs::remove_file(dir.join("group.txt")).ok();
        std::fs::remove_file(dir.join("group_group.txt")).ok();
        let result = load_map_spawns(41, dir.join("map").to_str().expect("utf8"));
        std::fs::remove_dir_all(&dir).ok();
        assert!(result.is_err(), "sin group.txt -> Err (parity shutdown)");
    }

    /// Tokenizer del loader de grupos: comillas, `#` comentario, vacías.
    #[test]
    fn loader_line_tokens_quotes_and_comments() {
        assert_eq!(
            loader_line_tokens("Leader\t\"Hungriger Wildhund\"\t171"),
            Some(vec!["Leader".into(), "Hungriger Wildhund".into(), "171".into()]),
            "token entre comillas sin las comillas (parity SplitLine)"
        );
        assert_eq!(loader_line_tokens("# comentario"), None);
        assert_eq!(loader_line_tokens("   "), None);
        assert_eq!(
            loader_line_tokens("Vnum\t101"),
            Some(vec!["Vnum".into(), "101".into()])
        );
    }

    /// Comentarios inline y líneas en blanco: transparentes (parity get_word).
    #[test]
    fn regen_tokens_skips_comments_and_blank_lines() {
        let text = "m\t1\t2\t0\t0\t0\t0\t1m\t100\t1\t3 // trailing\n\n// full line\nm\t4\t5\t0\t0\t0\t0\t2m\t100\t2\t6";
        let tokens = regen_tokens(text);
        assert_eq!(tokens.len(), 22, "{tokens:?}");
        let mut i = 0;
        let a = parse_regen_record(&tokens, &mut i).expect("a").expect("some");
        assert_eq!((a.c_x, a.c_y, a.vnum), (1, 2, 3));
        let b = parse_regen_record(&tokens, &mut i).expect("b").expect("some");
        assert_eq!((b.c_x, b.c_y, b.vnum, b.max_count), (4, 5, 6, 2));
        assert_eq!(parse_regen_record(&tokens, &mut i).expect("eof"), None, "EOF");
    }

    /// Tipos del regen + time (parity read_line / regen_load:682-693).
    #[test]
    fn regen_kinds_and_time() {
        // m -> Mob; ma -> Mob agresivo (el sufijo 'a' se ignora — parity
        // regen.cpp:124-126); g -> Group; r -> GroupGroup; s -> Anywhere.
        for (tok, kind) in [
            ("m", SpawnKind::Mob),
            ("ma", SpawnKind::Mob),
            ("g", SpawnKind::Group),
            ("r", SpawnKind::GroupGroup),
            ("s", SpawnKind::Anywhere),
        ] {
            let text = format!("{tok}\t1\t2\t0\t0\t0\t0\t1m\t100\t1\t9\n");
            let tokens = regen_tokens(&text);
            let mut i = 0;
            let rec = parse_regen_record(&tokens, &mut i).expect("rec").expect("some");
            assert_eq!(rec.kind, kind, "tipo '{tok}'");
        }
        // e -> exception: no es un spawn (Ok(None)).
        let tokens = regen_tokens("e\t1\t2\t0\t0\t0\n");
        let mut i = 0;
        assert_eq!(parse_regen_record(&tokens, &mut i).expect("exc"), None);
        // Tipo desconocido -> Err (parity exit(1)).
        let tokens = regen_tokens("x\t1\t2\t0\t0\t0\t0\t1m\t100\t1\t9\n");
        let mut i = 0;
        assert!(parse_regen_record(&tokens, &mut i).is_err());
        // Time: "1m" = 60, "7200s" = 7200, "5s" = 5, "100" (sin unidad) = 0 —
        // el 0 no spawnea al boot (regen_load:682-693).
        assert_eq!(parse_time("1m").unwrap(), 60);
        assert_eq!(parse_time("7200s").unwrap(), 7200);
        assert_eq!(parse_time("5s").unwrap(), 5);
        assert_eq!(parse_time("1h30m").unwrap(), 5400);
        assert_eq!(parse_time("100").unwrap(), 0);
        assert!(parse_time("10x").is_err());
    }

    /// entry_spawns — NPC (type 1): add 37 B + addInfo 70 B; monster (0):
    /// solo add 37 B (parity char.cpp:922 — el C++ no manda addInfo a mobs).
    #[test]
    fn entry_spawns_npc_vs_monster() {
        // NPC real del mapa 41: 20340 "Maestro Fuerza Corporal" (type=1).
        let npc = SpawnEntry { vnum: 20340, x: 966000, y: 267100, count: 1, kind: SpawnKind::Mob, w_x: 0, w_y: 0, time: 0 };
        let row = mob_row(20340, 1, &[0xB9, 0xD6, 0xBC, 0xBC]);
        let pkts = entry_spawns(41, &[(npc, row)], 10);
        assert_eq!(pkts.len(), 2, "NPC -> add + addInfo");
        assert_eq!(pkts[0].len(), TPacketGCCharacterAdd::SIZE, "37 B");
        assert_eq!(pkts[0][0], TPacketGCCharacterAdd::HEADER, "header 1");
        assert_eq!(pkts[1].len(), TPacketGCCharacterAdditionalInfo::SIZE, "70 B");
        assert_eq!(pkts[1][0], TPacketGCCharacterAdditionalInfo::HEADER, "header 136");
        // Monster real del mapa 41: 5001 "Pirata Tanaka" (type=0).
        let mon = SpawnEntry { vnum: 5001, x: 969600, y: 278400, count: 1, kind: SpawnKind::Mob, w_x: 0, w_y: 0, time: 0 };
        let mon_row = mob_row(5001, 0, b"Pirata Tanaka");
        let pkts = entry_spawns(41, &[(mon, mon_row)], 10);
        assert_eq!(pkts.len(), 1, "monster -> solo add");
        assert_eq!(pkts[0][0], TPacketGCCharacterAdd::HEADER);
    }

    /// Count > 1: N copias con VIDs consecutivos (parity AllocVID) y el
    /// par (add, addInfo) POR MOB — el cliente bufferiza NPCs en
    /// s_kNetActorData (slot único) y los suelta con el addInfo del VID.
    #[test]
    fn entry_spawns_count_and_vid_order() {
        let npc = SpawnEntry { vnum: 20001, x: 950800, y: 286000, count: 3, kind: SpawnKind::Mob, w_x: 0, w_y: 0, time: 0 };
        let row = mob_row(20001, 1, b"Alquimista");
        let pkts = entry_spawns(41, &[(npc, row)], 100);
        assert_eq!(pkts.len(), 6, "3 copias x 2 paquetes");
        for (i, pair) in pkts.chunks(2).enumerate() {
            let add = &pair[0];
            let info = &pair[1];
            assert_eq!(&add[1..5], &(100u32 + i as u32).to_le_bytes(), "VID add {i}");
            assert_eq!(&info[1..5], &(100u32 + i as u32).to_le_bytes(), "VID addInfo {i}");
            assert_eq!(add[21], CHAR_TYPE_NPC, "bType@21");
            assert_eq!(&add[22..26], &20001u32.to_le_bytes(), "wRaceNum@22 = vnum");
            // x/y UNITS crudos en el wire (long@9/@13).
            assert_eq!(&add[9..13], &950800i32.to_le_bytes());
            assert_eq!(&add[13..17], &286000i32.to_le_bytes());
            assert_eq!(&add[17..21], &0i32.to_le_bytes(), "z = 0");
        }
    }

    /// El nombre del addInfo: bytes crudos del locale_name (CP949) con NUL
    /// — parity GetName() = szLocaleName (strlcpy de 25 bytes).
    #[test]
    fn entry_spawns_add_info_name_bytes() {
        let npc = SpawnEntry { vnum: 20340, x: 966000, y: 267100, count: 1, kind: SpawnKind::Mob, w_x: 0, w_y: 0, time: 0 };
        // "Maestro..." CP949 simulado: 3 bytes + resto del nombre corto.
        let row = mob_row(20340, 1, &[0xB9, 0xD6, 0xBC, 0xBC, 0x00]);
        let pkts = entry_spawns(41, &[(npc, row)], 1);
        assert_eq!(&pkts[1][5..9], &[0xB9, 0xD6, 0xBC, 0xBC], "name@5 bytes crudos");
        assert_eq!(pkts[1][9], 0, "NUL tras el nombre");
        // locale_name vacío -> name zeroed (defensivo).
        let row = mob_row(20340, 1, &[]);
        let pkts = entry_spawns(41, &[(npc, row)], 1);
        assert!(pkts[1][5..30].iter().all(|&b| b == 0), "name zeroed");
        // locale_name largo -> truncado a 24 + NUL (strlcpy).
        let long = vec![0xAA; 40];
        let row = mob_row(20340, 1, &long);
        let pkts = entry_spawns(41, &[(npc, row)], 1);
        assert!(pkts[1][5..29].iter().all(|&b| b == 0xAA), "24 bytes copiados");
        assert_eq!(pkts[1][29], 0, "NUL en el byte 24");
        assert_eq!(pkts[1][30], 0, "el resto a cero");
    }

    /// TRAP grupos: entry_spawns omite Group/GroupGroup (los vnums de grupo
    /// colisionan con mobs reales — p.ej. 101 = group-group a1_01 Y mob
    /// Perro Salvaje); Anywhere SÍ emite (es un mob directo).
    #[test]
    fn entry_spawns_skips_group_kinds() {
        let gg = SpawnEntry { vnum: 101, x: 957600, y: 247300, count: 1, kind: SpawnKind::GroupGroup, w_x: 0, w_y: 0, time: 0 };
        let row = mob_row(101, 0, b"Perro Salvaje");
        assert!(entry_spawns(41, &[(gg, row.clone())], 1).is_empty(), "GroupGroup omitido");
        let g = SpawnEntry { vnum: 318, x: 959000, y: 315900, count: 1, kind: SpawnKind::Group, w_x: 0, w_y: 0, time: 0 };
        assert!(entry_spawns(41, &[(g, row.clone())], 1).is_empty(), "Group omitido");
        let any = SpawnEntry { vnum: 5001, x: 969600, y: 278400, count: 1, kind: SpawnKind::Anywhere, w_x: 0, w_y: 0, time: 0 };
        let pkts = entry_spawns(41, &[(any, row)], 1);
        assert_eq!(pkts.len(), 1, "Anywhere SÍ emite (mob directo)");
        assert_eq!(&pkts[0][22..26], &5001u32.to_le_bytes());
    }

    /// missing_vnums (la resolución batch): vnums DISTINTOS de las entradas
    /// spawnables no cacheados; grupos excluidos; duplicados colapsados.
    #[test]
    fn cache_missing_vnums_dedup_and_filter() {
        use std::collections::HashMap;
        let e = |vnum: u32, kind: SpawnKind| SpawnEntry { vnum, x: 0, y: 0, count: 1, kind, w_x: 0, w_y: 0, time: 0 };
        let spawns = [
            e(101, SpawnKind::Mob),
            e(101, SpawnKind::Mob), // duplicado
            e(102, SpawnKind::Mob),
            e(5004, SpawnKind::Anywhere),
            e(318, SpawnKind::Group), // no spawnable
            e(12007, SpawnKind::GroupGroup), // no spawnable
        ];
        let cache: HashMap<i64, MobRow> = [(101, mob_row(101, 0, b"x"))].into_iter().collect();
        assert_eq!(missing_vnums(&spawns, &cache), vec![102, 5004], "101 en caché, dup colapsado, grupos fuera");
        assert!(missing_vnums(&spawns, &HashMap::new()).contains(&101));
        // Caché vacía -> todos los spawnables, sin duplicados.
        let all = missing_vnums(&spawns, &HashMap::new());
        assert_eq!(all.len(), 3, "{all:?}");
    }

    /// MobCache::clear invalida (el NOTIFY/reload futuro lo usa).
    #[test]
    fn cache_clear_resets() {
        let mut c = MobCache::new();
        assert!(c.rows.is_empty(), "nueva caché vacía");
        c.clear();
        assert!(c.rows.is_empty());
    }
}
